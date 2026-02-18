// multical21.rs — Kamstrup Multical 21 water meter data parsing

use crate::*;

/// Parse decrypted Multical 21 payload into a MeterReading.
/// Decrypted data layout (matching C++ reference):
///   [0..2]  = CRC-16 of [2..end]
///   [2]     = CI field (0x79 = compact, 0x78 = long)
///   [3..]   = frame data (offsets below are absolute from data[0])
pub fn parse_multical21(data: &[u8]) -> Option<MeterReading> {
    if data.len() < 3 {
        warn!("Multical21: Decrypted data too short ({} bytes)", data.len());
        return None;
    }

    // Verify CRC: data[0..2] = CRC of data[2..end]
    let read_crc = (data[1] as u16) << 8 | data[0] as u16;
    let calc_crc = crc16_en13757(&data[2..]);
    if read_crc != calc_crc {
        warn!("Multical21: CRC mismatch (read={:04X} calc={:04X})", read_crc, calc_crc);
        info!("Multical21: data[{}]: {:02X?}", data.len(), data);
        return None;
    }

    let ci = data[2];
    info!("Multical21: CI={:02X} CRC OK", ci);

    let now = Utc::now();
    let timestamp = now.timestamp();
    let timestamp_s = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let reading = match ci {
        0x79 => {
            info!("Multical21: parsing compact dataframe (CI=0x79)");
            if data.len() < 19 {
                warn!("Multical21: Compact frame too short ({} bytes)", data.len());
                None
            } else {
                // Parse compact frame (CI=0x79).
                // Absolute offsets from decrypted data start (matching C++ reference impl):
                //   [9..13]:  total volume (u32 LE, liters)
                //   [13..17]: target volume (u32 LE, liters)
                //   [17]:     flow temperature
                //   [18]:     ambient temperature
                Some(MeterReading {
                    total_m3: u32::from_le_bytes([data[9], data[10], data[11], data[12]]) as f32 / 1000.0,
                    target_m3: u32::from_le_bytes([data[13], data[14], data[15], data[16]]) as f32 / 1000.0,
                    flow_temp: data[17],
                    ambient_temp: data[18],
                    info_codes: data[4],
                    timestamp,
                    timestamp_s,
                })
            }
        }
        0x78 => {
            info!("Multical21: parsing compact dataframe (CI=0x78)");
            if data.len() < 30 {
                warn!("Multical21: Long frame too short ({} bytes)", data.len());
                None
            } else {
                // Parse long frame (CI=0x78).
                // Absolute offsets from decrypted data start (matching C++ reference):
                //   [10..14]: total volume (u32 LE, liters)
                //   [16..20]: target volume (u32 LE, liters)
                //   [23]:     flow temperature
                //   [29]:     ambient temperature
                Some(MeterReading {
                    total_m3: u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as f32 / 1000.0,
                    target_m3: u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as f32 / 1000.0,
                    flow_temp: data[23],
                    ambient_temp: data[29],
                    info_codes: data[4],
                    timestamp,
                    timestamp_s,
                })
            }
        }
        _ => {
            warn!("Multical21: Unknown CI field 0x{:02X}", ci);
            None
        }
    };
    info!("Multical21 parsed reading: {reading:#?}");
    reading
}
// EOF
