use kdapp::episode::TxOutputInfo;
use kdapp::pki::PubKey;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ScriptError {
    #[error("malformed script bytes")]
    MalformedScript,
    #[error("no outputs matched allowed policies")]
    NoMatchingOutputs,
    #[error("insufficient matching value: required {required}, found {found}")]
    InsufficientValue { required: u64, found: u64 },
    #[error("value overflow while aggregating outputs")]
    ValueOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentSummary {
    pub covered_value: u64,
    pub matched_outputs: usize,
}

pub fn normalize_script_bytes(bytes: &[u8]) -> Result<Vec<u8>, ScriptError> {
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    while idx < bytes.len() {
        let opcode = bytes[idx];
        idx += 1;
        match opcode {
            0x01..=0x4b => {
                let len = opcode as usize;
                if bytes.len() < idx + len {
                    return Err(ScriptError::MalformedScript);
                }
                normalized.push(opcode);
                normalized.extend_from_slice(&bytes[idx..idx + len]);
                idx += len;
            }
            0x4c => {
                if idx >= bytes.len() {
                    return Err(ScriptError::MalformedScript);
                }
                let len = bytes[idx] as usize;
                idx += 1;
                if bytes.len() < idx + len {
                    return Err(ScriptError::MalformedScript);
                }
                let data = &bytes[idx..idx + len];
                normalized.extend_from_slice(&canonicalize_push(data));
                idx += len;
            }
            0x4d => {
                if idx + 1 >= bytes.len() {
                    return Err(ScriptError::MalformedScript);
                }
                let len = u16::from_le_bytes([bytes[idx], bytes[idx + 1]]) as usize;
                idx += 2;
                if bytes.len() < idx + len {
                    return Err(ScriptError::MalformedScript);
                }
                let data = &bytes[idx..idx + len];
                normalized.extend_from_slice(&canonicalize_push(data));
                idx += len;
            }
            0x4e => {
                if idx + 3 >= bytes.len() {
                    return Err(ScriptError::MalformedScript);
                }
                let len = u32::from_le_bytes([bytes[idx], bytes[idx + 1], bytes[idx + 2], bytes[idx + 3]]) as usize;
                idx += 4;
                if bytes.len() < idx + len {
                    return Err(ScriptError::MalformedScript);
                }
                let data = &bytes[idx..idx + len];
                normalized.extend_from_slice(&canonicalize_push(data));
                idx += len;
            }
            _ => normalized.push(opcode),
        }
    }
    Ok(normalized)
}

fn canonicalize_push(data: &[u8]) -> Vec<u8> {
    match data.len() {
        0 => vec![0x00],
        1..=75 => {
            let mut out = Vec::with_capacity(1 + data.len());
            out.push(data.len() as u8);
            out.extend_from_slice(data);
            out
        }
        76..=255 => {
            let mut out = Vec::with_capacity(2 + data.len());
            out.push(0x4c);
            out.push(data.len() as u8);
            out.extend_from_slice(data);
            out
        }
        256..=65_535 => {
            let mut out = Vec::with_capacity(3 + data.len());
            out.push(0x4d);
            out.extend_from_slice(&(data.len() as u16).to_le_bytes());
            out.extend_from_slice(data);
            out
        }
        _ => {
            let mut out = Vec::with_capacity(5 + data.len());
            out.push(0x4e);
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(data);
            out
        }
    }
}

fn decode_small_int(op: u8) -> Option<u8> {
    match op {
        0x00 => Some(0),
        0x51..=0x60 => Some(op - 0x50),
        _ => None,
    }
}

fn parse_multisig(script: &[u8]) -> Option<(u8, Vec<Vec<u8>>, u8)> {
    if script.is_empty() {
        return None;
    }
    let mut idx = 0;
    let m = decode_small_int(script[idx])?;
    idx += 1;

    let mut keys = Vec::new();
    while idx < script.len() {
        let opcode = script[idx];
        if !(1..=75).contains(&opcode) {
            break;
        }
        let len = opcode as usize;
        idx += 1;
        if script.len() < idx + len {
            return None;
        }
        keys.push(script[idx..idx + len].to_vec());
        idx += len;
    }

    if idx >= script.len() {
        return None;
    }
    let n = decode_small_int(script[idx])?;
    idx += 1;
    if idx >= script.len() || script[idx] != 0xae {
        return None;
    }
    idx += 1;
    if idx != script.len() || n as usize != keys.len() {
        return None;
    }
    Some((m, keys, n))
}

fn matches_p2pk(script: &[u8], merchant_keys: &[[u8; 33]]) -> bool {
    if script.len() != 35 || script[0] != 33 || script[34] != 0xac {
        return false;
    }
    let key = &script[1..34];
    merchant_keys.iter().any(|candidate| candidate.as_slice() == key)
}

fn matches_guardian(script: &[u8], merchant_keys: &[[u8; 33]], guardian_keys: &[[u8; 33]]) -> bool {
    if guardian_keys.is_empty() {
        return false;
    }
    let (m, keys, n) = match parse_multisig(script) {
        Some(parts) => parts,
        None => return false,
    };
    if m == 0 || n == 0 || n < m {
        return false;
    }
    let mut merchant_count = 0;
    let mut guardian_count = 0;
    for key in &keys {
        if merchant_keys.iter().any(|candidate| candidate.as_slice() == key.as_slice()) {
            merchant_count += 1;
        } else if guardian_keys.iter().any(|candidate| candidate.as_slice() == key.as_slice()) {
            guardian_count += 1;
        } else {
            return false;
        }
    }
    merchant_count >= 1 && guardian_count >= 1
}

fn matches_taproot(script: &[u8], script_version: u16, merchant_xonly: &[[u8; 32]]) -> bool {
    if script_version != 1 {
        return false;
    }
    if script.len() == 33 && script[0] == 0x20 {
        let key = &script[1..];
        return merchant_xonly.iter().any(|candidate| candidate.as_slice() == key);
    }
    if script.len() == 34 && script[0] == 0x51 && script[1] == 0x20 {
        let key = &script[2..];
        return merchant_xonly.iter().any(|candidate| candidate.as_slice() == key);
    }
    false
}

pub fn enforce_payment_policy(
    outputs: &[TxOutputInfo],
    required_amount: u64,
    merchant_keys: &[PubKey],
    guardian_keys: &[PubKey],
) -> Result<PaymentSummary, ScriptError> {
    let merchant_serialized: Vec<[u8; 33]> = merchant_keys.iter().map(|k| k.0.serialize()).collect();
    let guardian_serialized: Vec<[u8; 33]> = guardian_keys.iter().map(|k| k.0.serialize()).collect();
    let merchant_xonly: Vec<[u8; 32]> = merchant_keys
        .iter()
        .map(|k| k.0.x_only_public_key().0.serialize())
        .collect();

    let mut total_value = 0u64;
    let mut matched_outputs = 0usize;

    for output in outputs {
        let script_bytes = match &output.script_bytes {
            Some(bytes) => bytes,
            None => continue,
        };
        let normalized = normalize_script_bytes(script_bytes)?;
        let matched = matches_p2pk(&normalized, &merchant_serialized)
            || matches_guardian(&normalized, &merchant_serialized, &guardian_serialized)
            || matches_taproot(&normalized, output.script_version, &merchant_xonly);
        if matched {
            total_value = total_value
                .checked_add(output.value)
                .ok_or(ScriptError::ValueOverflow)?;
            matched_outputs += 1;
        }
    }

    if matched_outputs == 0 {
        return Err(ScriptError::NoMatchingOutputs);
    }

    if total_value < required_amount {
        return Err(ScriptError::InsufficientValue { required: required_amount, found: total_value });
    }

    Ok(PaymentSummary { covered_value: total_value, matched_outputs })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kdapp::episode::TxOutputInfo;
    use kdapp::pki::generate_keypair;

    #[test]
    fn normalizes_pushdata_variants() {
        let (_sk, pk) = generate_keypair();
        let mut script = vec![0x4c, 33];
        script.extend_from_slice(&pk.0.serialize());
        script.push(0xac);
        let normalized = normalize_script_bytes(&script).expect("normalize");
        assert_eq!(normalized[0], 33);
        assert_eq!(&normalized[1..34], &pk.0.serialize());
        assert_eq!(normalized[34], 0xac);
    }

    #[test]
    fn aggregates_multiple_outputs() {
        let ((_sk_m, merchant), (_sk_p, payer)) = (generate_keypair(), generate_keypair());
        let script = {
            let mut s = Vec::new();
            s.push(33);
            s.extend_from_slice(&merchant.0.serialize());
            s.push(0xac);
            s
        };
        let outputs = vec![
            TxOutputInfo { value: 40, script_version: 0, script_bytes: Some(script.clone()) },
            TxOutputInfo { value: 30, script_version: 0, script_bytes: Some(script) },
        ];
        let summary = enforce_payment_policy(&outputs, 60, &[merchant], &[]).expect("policy");
        assert_eq!(summary.covered_value, 70);
        assert_eq!(summary.matched_outputs, 2);
        // Non-matching invoice amount should fail
        let err = enforce_payment_policy(&outputs, 80, &[merchant], &[]).unwrap_err();
        assert!(matches!(err, ScriptError::InsufficientValue { .. }));
        // Ensure unused payer variable doesn't warn
        let _ = payer;
    }

    #[test]
    fn taproot_requires_version_one() {
        let (_sk, merchant) = generate_keypair();
        let xonly = merchant.0.x_only_public_key().0.serialize();
        let mut script = vec![0x51, 0x20];
        script.extend_from_slice(&xonly);

        let legacy_output = [TxOutputInfo { value: 1, script_version: 0, script_bytes: Some(script.clone()) }];
        let err = enforce_payment_policy(&legacy_output, 1, &[merchant], &[]).unwrap_err();
        assert_eq!(err, ScriptError::NoMatchingOutputs);

        let taproot_output = [TxOutputInfo { value: 1, script_version: 1, script_bytes: Some(script) }];
        let summary = enforce_payment_policy(&taproot_output, 1, &[merchant], &[]).expect("policy");
        assert_eq!(summary.matched_outputs, 1);
    }
}
