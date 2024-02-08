#![allow(clippy::collapsible_else_if)]

#[macro_use]
extern crate ic_cdk_macros;
#[macro_use]
extern crate serde;

mod types;

use base64::encode;
use candid::{Encode, Principal};
use ic_cdk::{
    api::{self, call},
    export::candid,
    storage,
};
use image::{ImageOutputFormat, Luma};
use include_base64::include_base64;
use qrcode::QrCode;
use serde_json::json;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::io::Cursor;
use std::iter::FromIterator;
use std::mem;
use types::{
    ConstrainedError, Error, InitArgs, InterfaceId, LogoResult, MetadataDesc, MintResult, Nft,
    Result, StableState, State,
};

const MGMT: Principal = Principal::from_slice(&[]);

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

//Prepares and serializes the canister's state before an upgrade, ensuring no data is lost during the upgrade process.
#[pre_upgrade]
fn pre_upgrade() {
    let state = STATE.with(|state| mem::take(&mut *state.borrow_mut()));
    let stable_state = StableState { state };
    storage::stable_save((stable_state,)).unwrap();
}
//Restores the canister's state after an upgrade, ensuring continuity of the canister's operation with the same data as before.
#[post_upgrade]
fn post_upgrade() {
    let (StableState { state },) = storage::stable_restore().unwrap();
    STATE.with(|state0| *state0.borrow_mut() = state);
}

#[init]
fn init(args: InitArgs) {
    let url = args.base_url.clone();
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.custodians = args
            .custodians
            .unwrap_or_else(|| HashSet::from_iter([api::caller()]));
        state.name = args.name;
        state.symbol = args.symbol;
        state.logo = args.logo;
        state.base_url = url;
    });
    // Set base url for redirect
}

// --------------
// change base url
// --------------
#[update(name = "set_base_url")]
fn set_base_url(url: String) {
    // Check if user is custodian

    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.custodians.contains(&api::caller()) {
            state.base_url = url;
        }
    });
}

// --------------
// base interface
// --------------

#[query(name = "balanceOfDip721")]
fn balance_of(user: Principal) -> u64 {
    STATE.with(|state| {
        state
            .borrow()
            .nfts
            .iter()
            .filter(|n| n.owner == user)
            .count() as u64
    })
}

// --------------
// change nft info
// --------------
#[update(name = "change_nft_info")]
fn change_nft_info(metadata: MetadataDesc) -> Result<String, Error> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let state = &mut *state;
        let nft = state
            .nfts
            .iter_mut()
            .find(|n| n.owner == api::caller())
            // .get_mut(usize::try_from(token_id)?)
            .ok_or(Error::InvalidTokenId)?;
        let caller = api::caller();
        if nft.owner != caller {
            Err(Error::Unauthorized)
        } else {
            nft.approved = None;
            nft.metadata = metadata;
            Ok("Transaction successful".to_string())
        }
    })
}

#[query(name = "ownerOfDip721")]
fn owner_of(token_id: u64) -> Result<Principal> {
    STATE.with(|state| {
        let owner = state
            .borrow()
            .nfts
            .get(usize::try_from(token_id)?)
            .ok_or(Error::InvalidTokenId)?
            .owner;
        Ok(owner)
    })
}

#[update(name = "transferFromDip721")]
fn transfer_from(from: Principal, to: Principal, token_id: u64) -> Result {
    //Transferring is not allowed since it's a social nft
    return Err(Error::Unauthorized);
}

#[query(name = "supportedInterfacesDip721")]
fn supported_interfaces() -> &'static [InterfaceId] {
    &[InterfaceId::Mint]
}

#[export_name = "canister_query logoDip721"]
fn logo() /* -> &'static LogoResult */
{
    ic_cdk::setup();
    STATE.with(|state| call::reply((state.borrow().logo.as_ref().unwrap_or(&DEFAULT_LOGO),)))
}

#[query(name = "nameDip721")]
fn name() -> String {
    STATE.with(|state| state.borrow().name.clone())
}

#[query(name = "symbolDip721")]
fn symbol() -> String {
    STATE.with(|state| state.borrow().symbol.clone())
}

const DEFAULT_LOGO: LogoResult = LogoResult {
    data: Cow::Borrowed(include_base64!("logo.png")),
    logo_type: Cow::Borrowed("image/png"),
};

#[query(name = "totalSupplyDip721")]
fn total_supply() -> u64 {
    STATE.with(|state| state.borrow().nfts.len() as u64)
}

#[query(name = "getMetadataDip721")]
fn get_metadata(token_id: u64) -> Result<String, Error> {
    ic_cdk::setup();
    STATE.with(|state| {
        let state = state.borrow();
        let metadata = &state
            .nfts
            .get(usize::try_from(token_id)?)
            .ok_or(Error::InvalidTokenId)?
            .metadata
            .clone();

        Ok(json!(metadata).to_string())
    })
}

#[query(name = "getMetadataForUserDip721")]
fn get_metadata_for_user() -> String {
    ic_cdk::setup();

    // let user = call::arg_data::<(Principal,)>().0;
    STATE
        .with(|state| {
            let state = state.borrow();
            let token_id;
            if let Some(nft) = state.nfts.iter().find(|n| n.owner == ic_cdk::api::caller()) {
                token_id = nft.id;
            } else {
                return Err("No NFT found for user".to_string());
            }
            // Concat url with nft.id
            let url = format!("{}/nft/{}", state.base_url, token_id);
            let code = QrCode::new(url.as_bytes()).map_err(|_| "Failed to create QR code")?;

            let image = code.render::<Luma<u8>>().build();

            // Use a Cursor wrapped around a Vec<u8> to provide a buffer with Seek + Write
            let mut png_bytes = Vec::new();
            let mut cursor = Cursor::new(&mut png_bytes);

            // Write the image data to the cursor
            image
                .write_to(&mut cursor, ImageOutputFormat::Png)
                .map_err(|_| "Failed to write image to bytes")?;

            // Encode the byte vector (now filled with the PNG data) into a base64 string
            let base64_image = encode(&png_bytes); // Encode directly from Vec<u8> without needing to dereference cursor

            return Ok(format!("{} {}", "data:image/png;base64,", base64_image));
        })
        .unwrap_or_else(|e| e)
}

// --------------
// mint interface
// --------------

#[update(name = "mintDip721")]
fn mint(
    // _to: Principal,
    metadata: MetadataDesc,
) -> Result<MintResult, ConstrainedError> {
    let (txid, tkid) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        // Everyone can mint, but only one NFT per user
        if let Some(_nft) = state.nfts.iter().find(|n| n.owner == ic_cdk::api::caller()) {
            return Err(ConstrainedError::AlreadyExists);
        }
        let new_id = state.nfts.len() as u64;
        let nft = Nft {
            owner: ic_cdk::api::caller(), // Caller will be owner
            approved: None,
            id: new_id,
            metadata,
        };
        state.nfts.push(nft);
        Ok((state.next_txid(), new_id))
    })?;
    Ok(MintResult {
        id: txid,
        token_id: tkid,
    })
}

// --------------
// burn interface
// --------------

#[update(name = "burnDip721")]
fn burn(token_id: u64) -> Result {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let nft = state
            .nfts
            .get_mut(usize::try_from(token_id)?)
            .ok_or(Error::InvalidTokenId)?;
        if nft.owner != api::caller() {
            Err(Error::Unauthorized)
        } else {
            nft.owner = MGMT;
            Ok(state.next_txid())
        }
    })
}

#[update]
fn set_name(name: String) -> Result<()> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.custodians.contains(&api::caller()) {
            state.name = name;
            Ok(())
        } else {
            Err(Error::Unauthorized)
        }
    })
}

#[update]
fn set_symbol(sym: String) -> Result<()> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.custodians.contains(&api::caller()) {
            state.symbol = sym;
            Ok(())
        } else {
            Err(Error::Unauthorized)
        }
    })
}

#[update]
fn set_logo(logo: Option<LogoResult>) -> Result<()> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.custodians.contains(&api::caller()) {
            state.logo = logo;
            Ok(())
        } else {
            Err(Error::Unauthorized)
        }
    })
}

#[update]
fn set_custodian(user: Principal, custodian: bool) -> Result<()> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.custodians.contains(&api::caller()) {
            if custodian {
                state.custodians.insert(user);
            } else {
                state.custodians.remove(&user);
            }
            Ok(())
        } else {
            Err(Error::Unauthorized)
        }
    })
}

#[query]
fn is_custodian(principal: Principal) -> bool {
    STATE.with(|state| state.borrow().custodians.contains(&principal))
}
