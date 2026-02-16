// wmbus.rs — wMBus frame decoding, CRC-16, AES-128-CTR decryption

use aes::Aes128;
use ctr::cipher::{KeyIvInit, StreamCipher};
use ctr::Ctr128BE;

use crate::*;

/// CRC-16 EN 13757 (polynomial 0x3D65, init 0x0000, final XOR 0xFFFF, no reflection)
pub fn crc16_en13757(data: &[u8]) -> u16 {
    let mut crc: u16 = 0x0000;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x3D65;
            } else {
                crc <<= 1;
            }
        }
    }
    crc ^ 0xFFFF
}

/// Check if payload meter ID matches expected meter ID.
/// Meter serial is at payload[4..8] in little-endian BCD, reversed vs printed serial.
pub fn check_meter_id(payload: &[u8], meter_id: &[u8; 4]) -> bool {
    if payload.len() < 8 {
        return false;
    }
    // Compare bytes directly — meter_id should already be in wire order
    payload[4] == meter_id[0]
        && payload[5] == meter_id[1]
        && payload[6] == meter_id[2]
        && payload[7] == meter_id[3]
}

/// Construct AES-128-CTR IV for ELL-II (CI=0x8D) from wMBus frame header.
/// IV layout (16 bytes):
///   [0..2]   = manufacturer (M-field, raw[2..4])
///   [2..8]   = address (A-field: serial[4] + version + type, raw[4..10])
///   [8]      = CC (Communication Control, raw[11])
///   [9..13]  = SN (Session Number, raw[13..17])
///   [13..16] = 0x00 (padding)
fn build_iv(raw: &[u8]) -> [u8; 16] {
    let mut iv = [0u8; 16];
    iv[0..2].copy_from_slice(&raw[2..4]); // M-field
    iv[2..8].copy_from_slice(&raw[4..10]); // A-field (serial + version + type)
    iv[8] = raw[11]; // CC
    iv[9..13].copy_from_slice(&raw[13..17]); // SN
    iv
}

/// Decrypt ELL-II wMBus payload using AES-128-CTR.
/// For CI=0x8D: encrypted data starts at raw[17], length = L - 2 - 16 bytes.
fn decrypt_payload(raw: &[u8], key: &[u8; 16]) -> Option<Vec<u8>> {
    let l_field = raw[0] as usize;
    // Encrypted data: raw[17..L-1] (skip 17-byte header, exclude 2 trailing bytes)
    // Matches reference: cipherLength = length - 2 - 16
    let encrypted_start = 17;
    let encrypted_end = l_field.checked_sub(1)?;

    if encrypted_start >= encrypted_end || encrypted_end > raw.len() {
        warn!(
            "wMBus: No encrypted data (start={}, end={}, len={})",
            encrypted_start, encrypted_end, raw.len()
        );
        return None;
    }

    let iv = build_iv(raw);
    let mut decrypted = raw[encrypted_start..encrypted_end].to_vec();

    let mut cipher = Ctr128BE::<Aes128>::new(key.into(), &iv.into());
    cipher.apply_keystream(&mut decrypted);

    Some(decrypted)
}

/// Full wMBus frame parsing pipeline: check meter ID → decrypt → parse.
pub fn parse_frame(raw: &[u8], meter_id: &[u8; 4], key: &[u8; 16]) -> Option<MeterReading> {
    if raw.len() < 18 {
        warn!("wMBus: Frame too short ({} bytes)", raw.len());
        return None;
    }

    let c_field = raw[1];
    if c_field != 0x44 {
        return None;
    }

    if !check_meter_id(raw, meter_id) {
        info!(
            "wMBus: Ignoring meter {:02X}{:02X}{:02X}{:02X}",
            raw[7], raw[6], raw[5], raw[4]
        );
        return None;
    }

    // CI=0x8D: ELL-II (encrypted)
    //   [10] CI  [11] CC  [12] ACC  [13..17] SN (4 bytes)  [17+] encrypted
    if raw[10] != 0x8D {
        warn!("wMBus: Unsupported CI field: 0x{:02X}", raw[10]);
        return None;
    }

    let decrypted = decrypt_payload(raw, key)?;
    crate::multical21::parse_multical21(&decrypted)
}
// EOF
