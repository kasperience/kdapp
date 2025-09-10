# NFC and BLE Invoice Encoding

This document describes the minimal data formats used when exchanging
invoice requests via NFC tags or BLE characteristics.

## NFC

Invoice requests are encoded as a single NDEF URI record using the
`onlykas` scheme:

```
onlykas://invoice/{invoice_id}?amount={amount}&memo={memo}
```

Example:

```
onlykas://invoice/42?amount=2500&memo=coffee
```

A wallet or web application reads the URI from the NDEF record and
parses the invoice id, amount and optional memo. The helper functions in
`examples/kdapp-merchant/src/tap.rs` provide `encode_ndef` and
`decode_ndef` utilities.

## BLE

BLE devices expose a characteristic containing a TLV encoded
`CreateInvoice` request. The bytes follow the `TlvMsg` structure defined
in `examples/kdapp-merchant/src/tlv.rs` with `msg_type = Cmd`. The TLV
payload is the Borsh serialization of the
`MerchantCommand::CreateInvoice` variant.

Applications can generate the characteristic bytes using
`encode_ble` and parse them with `decode_ble`.

Example usage in Rust:

```rust
use kdapp_merchant::tap::{InvoiceRequest, encode_ble, decode_ble};

let req = InvoiceRequest { invoice_id: 7, amount: 10_000, memo: Some("latte".into()) };
let bytes = encode_ble(&req); // send over BLE
let parsed = decode_ble(&bytes).unwrap();
assert_eq!(req, parsed);
```

These formats allow wallets to interact with merchant terminals without
platform-specific code, preparing the path for future mobile
implementations.
