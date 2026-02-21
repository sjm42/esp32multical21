// radio.rs — CC1101 SPI radio driver for wMBus C1 mode

use cc1101::{
    Cc1101,
    lowlevel::{
        Cc1101 as LowLevelCc1101,
        registers::{Command as CcCommand, Config as CcConfig, Status as CcStatus},
    },
};

use crate::*;

#[derive(Debug, thiserror::Error)]
pub enum Cc1101RadioError {
    #[error("CC1101 error: {0}")]
    Cc1101(#[from] cc1101::Error<spi::SpiError>),
    #[error("SPI error: {0}")]
    Spi(#[from] spi::SpiError),
    #[error("ESP-IDF error: {0}")]
    Esp(#[from] esp_idf_sys::EspError),
}

// SPI access mode bits
const READ_BURST: u8 = 0xC0;

// FIFO
const FIFO: u8 = 0x3F;

// MARCSTATE values
const MARC_IDLE: u8 = 0x01;
const MARC_RX: u8 = 0x0D;

// wMBus C1 mode register targets
const WMBUS_SYNC_WORD: u16 = 0x543D;
const WMBUS_IF_HZ: u64 = 203_125; // FSCTRL1 = 0x08
const WMBUS_FREQ_HZ: u64 = 868_949_708; // FREQ2/1/0 = 0x21,0x6B,0xD0
const WMBUS_CHANBW_HZ: u64 = 325_000; // MDMCFG4.CHANBW = 0b01_01
const WMBUS_DATA_RATE_BPS: u64 = 103_149; // MDMCFG3/4 = 0x04/0x5C
const WMBUS_DEVIATION_HZ: u64 = 34_913; // DEVIATN = 0x44

// https://www.ti.com/lit/ds/symlink/cc1101.pdf

const LEGACY_PROFILE: &[(CcConfig, u8)] = &[
    // (CcConfig::FIFOTHR, 0x00),
    // (CcConfig::IOCFG0, 0x06),
    (CcConfig::FIFOTHR, 0x01),
    (CcConfig::IOCFG0, 0x01),
    (CcConfig::IOCFG2, 0x2E),
    (CcConfig::SYNC1, 0x54),
    (CcConfig::SYNC0, 0x3D),
    (CcConfig::PKTLEN, 0x30),
    (CcConfig::PKTCTRL1, 0x00),
    (CcConfig::PKTCTRL0, 0x02),
    (CcConfig::ADDR, 0x00),
    (CcConfig::CHANNR, 0x00),
    (CcConfig::FSCTRL0, 0x00),
    (CcConfig::MDMCFG2, 0x06),
    (CcConfig::MDMCFG1, 0x22),
    (CcConfig::MDMCFG0, 0xF8),
    (CcConfig::MCSM1, 0x00),
    (CcConfig::MCSM0, 0x18),
    (CcConfig::FOCCFG, 0x2E),
    (CcConfig::BSCFG, 0xBF),
    (CcConfig::AGCCTRL2, 0x43),
    (CcConfig::AGCCTRL1, 0x09),
    (CcConfig::AGCCTRL0, 0xB5),
    (CcConfig::FREND1, 0xB6),
    (CcConfig::FREND0, 0x10),
    (CcConfig::FSCAL3, 0xEA),
    (CcConfig::FSCAL2, 0x2A),
    (CcConfig::FSCAL1, 0x00),
    (CcConfig::FSCAL0, 0x1F),
    (CcConfig::FSTEST, 0x59),
    (CcConfig::TEST2, 0x81),
    (CcConfig::TEST1, 0x35),
    (CcConfig::TEST0, 0x09),
    // all config overlapping with the corresponding high-level calls
    // are commented out, but retained here for future reference
    // (CcConfig::FSCTRL1, 0x08), // set_synthesizer_if()
    // (CcConfig::FREQ2, 0x21), // set_frequency()
    // (CcConfig::FREQ1, 0x6B), // set_frequency()
    // (CcConfig::FREQ0, 0xD0), // set_frequency()
    // (CcConfig::MDMCFG4, 0x5C), // set_chanbw(), set_data_rate()
    // (CcConfig::MDMCFG3, 0x04), // set_data_rate()
    // (CcConfig::DEVIATN, 0x44), // set_deviation()
];

// Radio watchdog timeout: restart if no packet in set time
const WATCHDOG_SECS: u64 = 600;

pub struct Cc1101Radio<'a> {
    spi: spi::SpiDeviceDriver<'a, &'a esp_idf_hal::spi::SpiDriver<'a>>,
    gdo0: PinDriver<'a, AnyInputPin, Input>,
}

impl<'a> Cc1101Radio<'a> {
    pub fn new(
        spi: spi::SpiDeviceDriver<'a, &'a esp_idf_hal::spi::SpiDriver<'a>>,
        gdo0: PinDriver<'a, AnyInputPin, Input>,
    ) -> Self {
        Self { spi, gdo0 }
    }

    fn write_config(&mut self, reg: CcConfig, value: u8) -> Result<(), Cc1101RadioError> {
        let mut radio = LowLevelCc1101::new(&mut self.spi)?;
        radio.write_register(reg, value)?;
        Ok(())
    }

    #[allow(dead_code)]
    fn read_config(&mut self, reg: CcConfig) -> Result<u8, Cc1101RadioError> {
        let mut radio = LowLevelCc1101::new(&mut self.spi)?;
        Ok(radio.read_register(reg)?)
    }

    fn read_status(&mut self, reg: CcStatus) -> Result<u8, Cc1101RadioError> {
        let mut radio = LowLevelCc1101::new(&mut self.spi)?;
        Ok(radio.read_register(reg)?)
    }

    fn strobe(&mut self, cmd: CcCommand) -> Result<(), Cc1101RadioError> {
        let mut radio = LowLevelCc1101::new(&mut self.spi)?;
        radio.write_strobe(cmd)?;
        Ok(())
    }

    fn read_fifo_burst(&mut self, buf: &mut [u8]) -> Result<(), Cc1101RadioError> {
        // First byte is the FIFO address with burst read flag
        let len = buf.len();
        let mut txbuf = vec![0u8; len + 1];
        txbuf[0] = FIFO | READ_BURST;
        self.spi.transfer_in_place(&mut txbuf)?;
        buf.copy_from_slice(&txbuf[1..]);
        Ok(())
    }

    fn start_receiver(&mut self) -> Result<(), Cc1101RadioError> {
        // Go to IDLE
        self.strobe(CcCommand::SIDLE)?;
        // Wait for IDLE state
        for _ in 0..35 {
            let state = self.read_status(CcStatus::MARCSTATE)? & 0x1F;
            if state == MARC_IDLE {
                break;
            }
            FreeRtos::delay_ms(3);
        }
        // Flush RX FIFO
        self.strobe(CcCommand::SFRX)?;
        // Start RX
        self.strobe(CcCommand::SRX)?;
        // Wait for RX state
        for _ in 0..35 {
            let state = self.read_status(CcStatus::MARCSTATE)? & 0x1F;
            if state == MARC_RX {
                break;
            }
            FreeRtos::delay_ms(3);
        }
        Ok(())
    }

    pub fn init(&mut self) -> Result<(), Cc1101RadioError> {
        info!("CC1101: Resetting radio...");
        {
            let mut radio = Cc1101::new(&mut self.spi)?;
            radio.reset()?;
        }
        FreeRtos::delay_ms(100);

        // Force exact legacy profile because some bit patterns are not expressible
        // via crate high-level enums (for example MDMCFG2 sync+carrier variants).
        info!("CC1101: Applying low-level config...");
        for (reg, value) in LEGACY_PROFILE {
            self.write_config(*reg, *value)?;
        }

        info!("CC1101: Applying high-level config...");
        {
            let mut radio = Cc1101::new(&mut self.spi)?;
            radio.set_synthesizer_if(WMBUS_IF_HZ)?;
            radio.set_frequency(WMBUS_FREQ_HZ)?;
            radio.set_chanbw(WMBUS_CHANBW_HZ)?;
            radio.set_data_rate(WMBUS_DATA_RATE_BPS)?;
            radio.set_deviation(WMBUS_DEVIATION_HZ)?;
        }

        // This check was only needed to be made once.
        // We are retaining the code in comments for reference.

        /*
        // Ensure final config bytes match legacy values exactly.
        for (reg, expected) in LEGACY_PROFILE {
            let got = self.read_config(*reg)?;
            if got != *expected {
                error!(
                    "CC1101 register mismatch {:?}: got 0x{:02X}, expected 0x{:02X}",
                    reg, got, expected
                );
            }
        }
        info!("CC1101 register checks done.");
        */

        // Calibrate
        self.strobe(CcCommand::SCAL)?;
        FreeRtos::delay_ms(100);

        // Verify chip
        let partnum = self.read_status(CcStatus::PARTNUM)?;
        let version = self.read_status(CcStatus::VERSION)?;
        info!("CC1101: PARTNUM=0x{:02X} VERSION=0x{:02X}", partnum, version);

        // Start receiving
        self.start_receiver()?;
        info!("CC1101: Radio initialized, listening");
        Ok(())
    }

    pub fn restart_radio(&mut self) -> Result<(), Cc1101RadioError> {
        warn!("CC1101: Restarting radio (watchdog)...");
        self.init()
    }

    /// Wait for a wMBus packet. Returns `Ok(None)` on watchdog timeout.
    pub async fn wait_for_packet(&mut self) -> Result<Option<Vec<u8>>, Cc1101RadioError> {
        match Box::pin(timeout(Duration::from_secs(WATCHDOG_SECS), self.poll_gdo0())).await {
            Ok(packet) => Ok(Some(packet?)),
            Err(_) => {
                warn!("CC1101: Watchdog timeout ({}s) with no packets received", WATCHDOG_SECS);
                Ok(None)
            }
        }
    }

    async fn poll_gdo0(&mut self) -> Result<Vec<u8>, Cc1101RadioError> {
        // IOCFG0=0x01 and FIFOTHR=0x01: GDO0 rises when FIFO has at least 8 bytes
        // IOCFG0=0x01 and FIFOTHR=0x0E: GDO0 rises when FIFO has at least 60 bytes
        loop {
            while self.gdo0.is_low() {
                sleep(Duration::from_millis(100)).await;
            }
            // wait for the packet to be completely received
            sleep(Duration::from_millis(10)).await;

            // Packet received, radio should now be in IDLE.
            // Read RXBYTES to see how much data we got.
            let rx_bytes = self.read_status(CcStatus::RXBYTES)? & 0x7F;
            if rx_bytes == 0 {
                error!("CC1101: GDO0 triggered but FIFO empty?");
                self.start_receiver()?;
                continue;
            }

            info!("CC1101: Packet received, {} bytes", rx_bytes);

            // Read all FIFO bytes
            let mut fifo_data = vec![0u8; rx_bytes as usize];
            self.read_fifo_burst(&mut fifo_data)?;

            // Restart receiver for next packet
            self.start_receiver()?;

            // Check preamble bytes
            if fifo_data.len() < 3 {
                warn!("CC1101: Packet too short ({} bytes)", fifo_data.len());
                continue;
            }

            let sync_hi = ((WMBUS_SYNC_WORD >> 8) & 0xFF) as u8;
            let sync_lo = (WMBUS_SYNC_WORD & 0xFF) as u8;
            if fifo_data[0] != sync_hi || fifo_data[1] != sync_lo {
                warn!(
                    "CC1101: Bad preamble: {:02X} {:02X} (expected {:02X} {:02X})",
                    fifo_data[0], fifo_data[1], sync_hi, sync_lo
                );
                continue;
            }

            // Strip preamble, return L-field + payload
            let payload = fifo_data[2..].to_vec();
            info!("CC1101: Valid wMBus packet, {} bytes", payload.len());
            return Ok(payload);
        }
    }
}
// EOF
