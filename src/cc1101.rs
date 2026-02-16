// cc1101.rs — CC1101 SPI radio driver for wMBus C1 mode

use crate::*;

// SPI access mode bits
#[allow(dead_code)]
const WRITE_BURST: u8 = 0x40;
const READ_SINGLE: u8 = 0x80;
const READ_BURST: u8 = 0xC0;

// Strobe commands
const SRES: u8 = 0x30;
const SCAL: u8 = 0x33;
const SRX: u8 = 0x34;
const SIDLE: u8 = 0x36;
const SFRX: u8 = 0x3A;

// Status registers (read with READ_BURST bit)
const MARCSTATE: u8 = 0x35;
const RXBYTES: u8 = 0x3B;

// FIFO
const FIFO: u8 = 0x3F;

// MARCSTATE values
const MARC_IDLE: u8 = 0x01;
const MARC_RX: u8 = 0x0D;

// CC1101 config register addresses
#[allow(dead_code)]
mod reg {
    pub const IOCFG2: u8 = 0x00;
    pub const IOCFG1: u8 = 0x01;
    pub const IOCFG0: u8 = 0x02;
    pub const FIFOTHR: u8 = 0x03;
    pub const SYNC1: u8 = 0x04;
    pub const SYNC0: u8 = 0x05;
    pub const PKTLEN: u8 = 0x06;
    pub const PKTCTRL1: u8 = 0x07;
    pub const PKTCTRL0: u8 = 0x08;
    pub const ADDR: u8 = 0x09;
    pub const CHANNR: u8 = 0x0A;
    pub const FSCTRL1: u8 = 0x0B;
    pub const FSCTRL0: u8 = 0x0C;
    pub const FREQ2: u8 = 0x0D;
    pub const FREQ1: u8 = 0x0E;
    pub const FREQ0: u8 = 0x0F;
    pub const MDMCFG4: u8 = 0x10;
    pub const MDMCFG3: u8 = 0x11;
    pub const MDMCFG2: u8 = 0x12;
    pub const MDMCFG1: u8 = 0x13;
    pub const MDMCFG0: u8 = 0x14;
    pub const DEVIATN: u8 = 0x15;
    pub const MCSM2: u8 = 0x16;
    pub const MCSM1: u8 = 0x17;
    pub const MCSM0: u8 = 0x18;
    pub const FOCCFG: u8 = 0x19;
    pub const BSCFG: u8 = 0x1A;
    pub const AGCCTRL2: u8 = 0x1B;
    pub const AGCCTRL1: u8 = 0x1C;
    pub const AGCCTRL0: u8 = 0x1D;
    pub const WOREVT1: u8 = 0x1E;
    pub const WOREVT0: u8 = 0x1F;
    pub const WORCTRL: u8 = 0x20;
    pub const FREND1: u8 = 0x21;
    pub const FREND0: u8 = 0x22;
    pub const FSCAL3: u8 = 0x23;
    pub const FSCAL2: u8 = 0x24;
    pub const FSCAL1: u8 = 0x25;
    pub const FSCAL0: u8 = 0x26;
    pub const RCCTRL1: u8 = 0x27;
    pub const RCCTRL0: u8 = 0x28;
    pub const FSTEST: u8 = 0x29;
    pub const PTEST: u8 = 0x2A;
    pub const AGCTEST: u8 = 0x2B;
    pub const TEST2: u8 = 0x2C;
    pub const TEST1: u8 = 0x2D;
    pub const TEST0: u8 = 0x2E;
}

// wMBus C1 mode preamble bytes (after sync word detection)
const PREAMBLE_0: u8 = 0x54;
const PREAMBLE_1: u8 = 0x3D;

// Radio watchdog timeout: restart if no packet in 5 minutes
const WATCHDOG_SECS: u64 = 300;

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

    fn write_register(&mut self, addr: u8, value: u8) {
        let mut buf = [addr, value];
        if let Err(e) = self.spi.transfer_in_place(&mut buf) {
            error!("CC1101 write_register 0x{:02X} error: {:?}", addr, e);
        }
    }

    #[allow(dead_code)]
    fn read_register(&mut self, addr: u8) -> u8 {
        let mut buf = [addr | READ_SINGLE, 0x00];
        if let Err(e) = self.spi.transfer_in_place(&mut buf) {
            error!("CC1101 read_register 0x{:02X} error: {:?}", addr, e);
            return 0;
        }
        buf[1]
    }

    fn read_status(&mut self, addr: u8) -> u8 {
        let mut buf = [addr | READ_BURST, 0x00];
        if let Err(e) = self.spi.transfer_in_place(&mut buf) {
            error!("CC1101 read_status 0x{:02X} error: {:?}", addr, e);
            return 0;
        }
        buf[1]
    }

    fn strobe(&mut self, cmd: u8) {
        let mut buf = [cmd];
        if let Err(e) = self.spi.transfer_in_place(&mut buf) {
            error!("CC1101 strobe 0x{:02X} error: {:?}", cmd, e);
        }
    }

    fn read_fifo_burst(&mut self, buf: &mut [u8]) {
        // First byte is the FIFO address with burst read flag
        let len = buf.len();
        let mut txbuf = vec![0u8; len + 1];
        txbuf[0] = FIFO | READ_BURST;
        if let Err(e) = self.spi.transfer_in_place(&mut txbuf) {
            error!("CC1101 read_fifo_burst error: {:?}", e);
            return;
        }
        buf.copy_from_slice(&txbuf[1..]);
    }

    fn start_receiver(&mut self) {
        // Go to IDLE
        self.strobe(SIDLE);
        // Wait for IDLE state
        for _ in 0..100 {
            let state = self.read_status(MARCSTATE) & 0x1F;
            if state == MARC_IDLE {
                break;
            }
            FreeRtos::delay_ms(1);
        }
        // Flush RX FIFO
        self.strobe(SFRX);
        // Start RX
        self.strobe(SRX);
        // Wait for RX state
        for _ in 0..100 {
            let state = self.read_status(MARCSTATE) & 0x1F;
            if state == MARC_RX {
                break;
            }
            FreeRtos::delay_ms(1);
        }
    }

    pub fn init(&mut self) {
        info!("CC1101: Resetting radio...");
        self.strobe(SRES);
        FreeRtos::delay_ms(10);

        info!("CC1101: Writing config registers...");
        self.write_register(reg::IOCFG2, 0x2E);
        self.write_register(reg::IOCFG0, 0x06); // GDO0: sync word, deassert end of packet
        self.write_register(reg::FIFOTHR, 0x00);
        self.write_register(reg::SYNC1, 0x54);
        self.write_register(reg::SYNC0, 0x3D); // wMBus C1 mode sync
        self.write_register(reg::PKTLEN, 0x30); // 48 bytes
        self.write_register(reg::PKTCTRL1, 0x00);
        self.write_register(reg::PKTCTRL0, 0x02); // infinite packet mode
        self.write_register(reg::ADDR, 0x00);
        self.write_register(reg::CHANNR, 0x00);
        self.write_register(reg::FSCTRL1, 0x08);
        self.write_register(reg::FSCTRL0, 0x00);
        self.write_register(reg::FREQ2, 0x21);
        self.write_register(reg::FREQ1, 0x6B);
        self.write_register(reg::FREQ0, 0xD0); // 868.3 MHz
        self.write_register(reg::MDMCFG4, 0x5C);
        self.write_register(reg::MDMCFG3, 0x04);
        self.write_register(reg::MDMCFG2, 0x06); // 2-FSK, 16/16 sync
        self.write_register(reg::MDMCFG1, 0x22);
        self.write_register(reg::MDMCFG0, 0xF8);
        self.write_register(reg::DEVIATN, 0x44);
        self.write_register(reg::MCSM1, 0x00); // after RX → IDLE
        self.write_register(reg::MCSM0, 0x18);
        self.write_register(reg::FOCCFG, 0x2E);
        self.write_register(reg::BSCFG, 0xBF);
        self.write_register(reg::AGCCTRL2, 0x43);
        self.write_register(reg::AGCCTRL1, 0x09);
        self.write_register(reg::AGCCTRL0, 0xB5);
        self.write_register(reg::FREND1, 0xB6);
        self.write_register(reg::FREND0, 0x10);
        self.write_register(reg::FSCAL3, 0xEA);
        self.write_register(reg::FSCAL2, 0x2A);
        self.write_register(reg::FSCAL1, 0x00);
        self.write_register(reg::FSCAL0, 0x1F);
        self.write_register(reg::FSTEST, 0x59);
        self.write_register(reg::TEST2, 0x81);
        self.write_register(reg::TEST1, 0x35);
        self.write_register(reg::TEST0, 0x09);

        // Calibrate
        self.strobe(SCAL);
        FreeRtos::delay_ms(100);

        // Verify chip
        let partnum = self.read_status(0x30); // PARTNUM
        let version = self.read_status(0x31); // VERSION
        info!(
            "CC1101: PARTNUM=0x{:02X} VERSION=0x{:02X}",
            partnum, version
        );

        // Start receiving
        self.start_receiver();
        info!("CC1101: Radio initialized, listening for wMBus packets...");
    }

    pub fn restart_radio(&mut self) {
        warn!("CC1101: Restarting radio (watchdog)...");
        self.init();
    }

    /// Wait for a wMBus packet. Returns payload bytes (after preamble) or None on timeout.
    pub async fn wait_for_packet(&mut self) -> Option<Vec<u8>> {
        // Wait for GDO0 to go high (sync detected) then low (packet done)
        // With IOCFG0=0x06 and MCSM1=0x00: GDO0 asserts on sync, deasserts when
        // radio goes to IDLE after packet reception.
        //
        // We use a watchdog timeout to restart the radio if stuck.

        let result = Box::pin(timeout(
            Duration::from_secs(WATCHDOG_SECS),
            self.wait_gdo0_packet(),
        ))
        .await;

        match result {
            Ok(pkt) => pkt,
            Err(_) => {
                warn!(
                    "CC1101: Watchdog timeout ({}s), no packet received",
                    WATCHDOG_SECS
                );
                None
            }
        }
    }

    async fn wait_gdo0_packet(&mut self) -> Option<Vec<u8>> {
        // Poll GDO0 for high→low transition indicating packet received
        // GDO0=0x06: asserts on sync word, deasserts when packet ends (IDLE)
        loop {
            // Wait for GDO0 to go high (sync detected)
            while self.gdo0.is_low() {
                sleep(Duration::from_millis(10)).await;
            }

            // GDO0 is high = sync word detected, wait for it to go low (packet complete)
            while self.gdo0.is_high() {
                sleep(Duration::from_millis(1)).await;
            }

            // Packet received, radio is now in IDLE
            // Read RXBYTES to see how much data
            let rx_bytes = self.read_status(RXBYTES) & 0x7F;
            if rx_bytes == 0 {
                warn!("CC1101: GDO0 triggered but FIFO empty");
                self.start_receiver();
                continue;
            }

            info!("CC1101: Packet received, {} bytes in FIFO", rx_bytes);

            // Read all FIFO bytes
            let mut fifo_data = vec![0u8; rx_bytes as usize];
            self.read_fifo_burst(&mut fifo_data);

            // Restart receiver for next packet
            self.start_receiver();

            // Check preamble bytes
            if fifo_data.len() < 3 {
                warn!("CC1101: Packet too short ({} bytes)", fifo_data.len());
                continue;
            }

            if fifo_data[0] != PREAMBLE_0 || fifo_data[1] != PREAMBLE_1 {
                warn!(
                    "CC1101: Bad preamble: {:02X} {:02X} (expected {:02X} {:02X})",
                    fifo_data[0], fifo_data[1], PREAMBLE_0, PREAMBLE_1
                );
                continue;
            }

            // Strip preamble, return L-field + payload
            let payload = fifo_data[2..].to_vec();
            info!(
                "CC1101: Valid wMBus packet, {} payload bytes",
                payload.len()
            );
            return Some(payload);
        }
    }
}
// EOF
