#!/usr/bin/env bash
dfx stop
set -e
trap 'dfx stop' EXIT

dfx start --background --clean
dfx deploy --argument 'record{name="DFX Blobs";symbol="DFXB";custodians=null;logo=null; base_url="https://example.com";}' dip721_nft_container
YOU=$(dfx identity get-principal)

echo '(*) Creating NFT with metadata "hello":'
dfx canister call dip721_nft_container mintDip721 \
    "(record{
        twitter=opt \"twitter.com/BayramUtku\";
        instagram = null;
        website = null;
        facebook = null;
        discord = null;
    })"
echo "(*) Owner of NFT 0 (you are $YOU):"
dfx canister call dip721_nft_container ownerOfDip721 '(0:nat64)'
echo '(*) Number of NFTs you own:'
dfx canister call dip721_nft_container balanceOfDip721 "(principal\"$YOU\")"
echo '(*) Total NFTs in existence:'
dfx canister call dip721_nft_container totalSupplyDip721
echo '(*) Metadata of the newly created NFT:'
dfx canister call dip721_nft_container getMetadataDip721 '(0:nat64)'
echo 'Changing metadata of NFT 0 and add a website:'
dfx canister call dip721_nft_container change_nft_info '(record {twitter=opt "twitter.com/BayramUtku"; website=opt "https://bu2.pw";instagram = null; facebook = null; discord = null;})'
echo '(*) Metadata of the newly created NFT:'
dfx canister call dip721_nft_container getMetadataDip721 '(0:nat64)'
echo '(*) Qr code of the newly created NFT:'
dfx canister call dip721_nft_container getMetadataForUserDip721 
