use borsh::{BorshDeserialize, BorshSerialize};

use crate::episode::MerchantCommand;
use crate::tlv::{MsgType, TlvMsg, TLV_VERSION};

/// Basic invoice request used for NFC/BLE taps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceRequest {
    pub invoice_id: u64,
    pub amount: u64,
    pub memo: Option<String>,
}

/// Encode the request as an NDEF URI record.
pub fn encode_ndef(req: &InvoiceRequest) -> String {
    let mut uri = format!("onlykas://invoice/{}?amount={}", req.invoice_id, req.amount);
    if let Some(m) = &req.memo {
        uri.push_str("&memo=");
        uri.push_str(m);
    }
    uri
}

/// Decode the request from an NDEF URI record.
pub fn decode_ndef(uri: &str) -> Option<InvoiceRequest> {
    const PREFIX: &str = "onlykas://invoice/";
    if !uri.starts_with(PREFIX) {
        return None;
    }
    let rest = &uri[PREFIX.len()..];
    let mut parts = rest.split('?');
    let id_part = parts.next()?;
    let invoice_id: u64 = id_part.parse().ok()?;
    let mut amount = None;
    let mut memo = None;
    if let Some(q) = parts.next() {
        for pair in q.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                match k {
                    "amount" => amount = v.parse().ok(),
                    "memo" => memo = Some(v.to_string()),
                    _ => {}
                }
            }
        }
    }
    Some(InvoiceRequest { invoice_id, amount: amount?, memo })
}

/// Encode the request into BLE characteristic bytes using TLV framing.
pub fn encode_ble(req: &InvoiceRequest) -> Vec<u8> {
    let cmd = MerchantCommand::CreateInvoice {
        invoice_id: req.invoice_id,
        amount: req.amount,
        memo: req.memo.clone(),
        guardian_keys: Vec::new(),
    };

    let payload = cmd.try_to_vec().expect("serialize command");

    let tlv = TlvMsg {
        version: TLV_VERSION,
        msg_type: MsgType::Cmd as u8,
        episode_id: 0,
        seq: 0,
        state_hash: [0u8; 32],
        payload,
        auth: [0u8; 32],
    };
    tlv.encode()
}

/// Decode the request from BLE characteristic bytes.
pub fn decode_ble(bytes: &[u8]) -> Option<InvoiceRequest> {
    let tlv = TlvMsg::decode(bytes)?;
    if MsgType::from_u8(tlv.msg_type)? != MsgType::Cmd {
        return None;
    }
    let cmd: MerchantCommand = MerchantCommand::try_from_slice(&tlv.payload).ok()?;
    if let MerchantCommand::CreateInvoice { invoice_id, amount, memo, .. } = cmd {
        Some(InvoiceRequest { invoice_id, amount, memo })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ndef_round_trip() {
        let req = InvoiceRequest { invoice_id: 42, amount: 25_000, memo: Some("coffee".into()) };
        let uri = encode_ndef(&req);
        let out = decode_ndef(&uri).expect("decode");
        assert_eq!(req, out);
    }

    #[test]
    fn ble_round_trip() {
        let req = InvoiceRequest { invoice_id: 7, amount: 10_000, memo: Some("latte".into()) };
        let bytes = encode_ble(&req);
        let out = decode_ble(&bytes).expect("decode");
        assert_eq!(req, out);
    }
}

