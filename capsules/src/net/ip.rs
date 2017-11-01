#[derive(Copy,Clone,PartialEq)]
pub enum MacAddr {
    ShortAddr(u16),
    LongAddr([u8; 8]),
}

pub mod ip6_nh {
    pub const HOP_OPTS: u8 = 0;
    pub const TCP: u8 = 6;
    pub const UDP: u8 = 17;
    pub const IP6: u8 = 41;
    pub const ROUTING: u8 = 43;
    pub const FRAGMENT: u8 = 44;
    pub const ICMP: u8 = 58;
    pub const NO_NEXT: u8 = 59;
    pub const DST_OPTS: u8 = 60;
    pub const MOBILITY: u8 = 135;
}

#[derive(Copy, Clone, Debug)]
pub struct IPAddr(pub [u8; 16]);

impl IPAddr {
    pub fn new() -> IPAddr {
        // Defaults to the unspecified address
        IPAddr([0; 16])
    }

    pub fn is_unspecified(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }

    pub fn is_unicast_link_local(&self) -> bool {
        self.0[0] == 0xfe && (self.0[1] & 0xc0) == 0x80 && (self.0[1] & 0x3f) == 0 &&
        self.0[2..8].iter().all(|&b| b == 0)
    }

    pub fn set_unicast_link_local(&mut self) {
        self.0[0] = 0xfe;
        self.0[1] = 0x80;
        for i in 2..8 {
            self.0[i] = 0;
        }
    }

    // Panics if prefix slice does not contain enough bits
    pub fn set_prefix(&mut self, prefix: &[u8], prefix_len: u8) {
        let full_bytes = (prefix_len / 8) as usize;
        let remaining = (prefix_len & 0x7) as usize;
        let bytes = full_bytes + (if remaining != 0 { 1 } else { 0 });
        assert!(bytes <= prefix.len() && bytes <= 16);

        self.0[0..full_bytes].copy_from_slice(&prefix[0..full_bytes]);
        if remaining != 0 {
            let mask = (0xff as u8) << (8 - remaining);
            self.0[full_bytes] &= !mask;
            self.0[full_bytes] |= mask & prefix[full_bytes];
        }
    }

    pub fn is_multicast(&self) -> bool {
        self.0[0] == 0xff
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct IP6Header {
    pub version_class_flow: [u8; 4],
    pub payload_len: u16,
    pub next_header: u8,
    pub hop_limit: u8,
    pub src_addr: IPAddr,
    pub dst_addr: IPAddr,
}

impl Default for IP6Header {
    fn default() -> IP6Header {
        let version = 0x60;
        let hop_limit = 255;
        IP6Header {
            version_class_flow: [version, 0, 0, 0],
            payload_len: 0,
            next_header: ip6_nh::NO_NEXT,
            hop_limit: hop_limit,
            src_addr: IPAddr::new(),
            dst_addr: IPAddr::new(),
        }
    }
}

impl IP6Header {
    pub fn new() -> IP6Header {
        IP6Header::default()
    }

    /// Parses a buffer into an `IP6Header`
    ///
    /// The buffer must be at least 40 bytes. This method performs no checking
    /// on the validity of the header.
    pub fn parse(buf: &[u8]) -> Result<IP6Header, ()> {
        if buf.len() < 40 {
            return Err(());
        }
        let version_class_flow: [u8; 4] = [buf[0], buf[1], buf[2], buf[3]];

        let payload_len: u16 = buf[4] as u16 | (buf[5] as u16) << 8;
        let next_header: u8 = buf[6];
        let hop_limit: u8 = buf[7];

        let mut src_addr = IPAddr::new();
        src_addr.0.copy_from_slice(&buf[8..][..16]);
        let mut dst_addr = IPAddr::new();
        dst_addr.0.copy_from_slice(&buf[24..][..16]);

        Ok(IP6Header {
            version_class_flow: version_class_flow,
            payload_len: payload_len,
            next_header: next_header,
            hop_limit: hop_limit,
            src_addr: src_addr,
            dst_addr: dst_addr,
        })
    }

    pub fn serialize(&self, buf: &mut [u8]) -> Result<(), ()> {
        if buf.len() < 40 {
            return Err(());
        }

        let mut buf = [0; 40];

        buf[0..4].copy_from_slice(&self.version_class_flow);
        buf[4] = self.payload_len as u8;
        buf[5] = (self.payload_len >> 8) as u8;
        buf[6] = self.next_header;
        buf[7] = self.hop_limit;

        buf[8..][..16].copy_from_slice(&self.src_addr.0);
        buf[24..][..16].copy_from_slice(&self.dst_addr.0);

        Ok(())

    }

    // Version should always be 6
    pub fn get_version(&self) -> u8 {
        (self.version_class_flow[0] & 0xf0) >> 4
    }

    // TODO: Confirm order
    pub fn get_traffic_class(&self) -> u8 {
        (self.version_class_flow[0] & 0x0f) << 4 | (self.version_class_flow[1] & 0xf0) >> 4
    }

    pub fn set_traffic_class(&mut self, new_tc: u8) {
        self.version_class_flow[0] &= 0xf0;
        self.version_class_flow[0] |= (new_tc & 0xf0) >> 4;
        self.version_class_flow[1] &= 0x0f;
        self.version_class_flow[1] |= (new_tc & 0x0f) << 4;
    }

    fn get_dscp_unshifted(&self) -> u8 {
        self.get_traffic_class() & 0b11111100
    }

    pub fn get_dscp(&self) -> u8 {
        self.get_dscp_unshifted() >> 2
    }

    pub fn set_dscp(&mut self, new_dscp: u8) {
        let ecn = self.get_ecn();
        self.set_traffic_class(ecn | ((new_dscp << 2) & 0b11111100));
    }

    pub fn get_ecn(&self) -> u8 {
        self.get_traffic_class() & 0b11
    }

    pub fn set_ecn(&mut self, new_ecn: u8) {
        let dscp_unshifted = self.get_dscp_unshifted();
        self.set_traffic_class(dscp_unshifted | (new_ecn & 0b11));
    }

    // This returns the flow label as the lower 20 bits of a u32
    pub fn get_flow_label(&self) -> u32 {
        let mut flow_label: u32 = 0;
        flow_label |= ((self.version_class_flow[1] & 0x0f) as u32) << 16;
        flow_label |= (self.version_class_flow[2] as u32) << 8;
        flow_label |= self.version_class_flow[3] as u32;
        flow_label
    }

    pub fn set_flow_label(&mut self, new_fl_val: u32) {
        self.version_class_flow[1] &= 0xf0;
        self.version_class_flow[1] |= ((new_fl_val >> 16) & 0x0f) as u8;
        self.version_class_flow[2] = (new_fl_val >> 8) as u8;
        self.version_class_flow[3] = new_fl_val as u8;
    }

    // TODO: Is this in network byte order?
    pub fn get_payload_len(&self) -> u16 {
        u16::from_be(self.payload_len)
    }

    // TODO: 40 = size of IP6header - find idiomatic way to compute
    pub fn get_total_len(&self) -> u16 {
        40 + self.get_payload_len()
    }

    // TODO: Is this in network byte order?
    pub fn set_payload_len(&mut self, new_len: u16) {
        self.payload_len = new_len.to_be();
    }

    pub fn get_next_header(&self) -> u8 {
        self.next_header
    }

    pub fn set_next_header(&mut self, new_nh: u8) {
        self.next_header = new_nh;
    }

    pub fn get_hop_limit(&self) -> u8 {
        self.hop_limit
    }

    pub fn set_hop_limit(&mut self, new_hl: u8) {
        self.hop_limit = new_hl;
    }
}
