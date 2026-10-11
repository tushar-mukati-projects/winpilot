use windows::core::GUID;

const GUID_BYTES: usize = 16;
const SHELL_CLOAK: u32 = 0x2;
const APP_CLOAK: u32 = 0x1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cloak {
    Shown,
    ByShell,
    Other,
}

pub(crate) fn parse_desktop_ids(bytes: &[u8]) -> Vec<GUID> {
    bytes.as_chunks::<GUID_BYTES>().0.iter().map(guid_of).collect()
}

pub(crate) fn guid_from_bytes(bytes: &[u8]) -> Option<GUID> {
    <&[u8; GUID_BYTES]>::try_from(bytes).ok().map(guid_of)
}

fn guid_of(bytes: &[u8; GUID_BYTES]) -> GUID {
    let [a0, a1, a2, a3, b0, b1, c0, c1, rest @ ..] = *bytes;
    GUID {
        data1: u32::from_le_bytes([a0, a1, a2, a3]),
        data2: u16::from_le_bytes([b0, b1]),
        data3: u16::from_le_bytes([c0, c1]),
        data4: rest,
    }
}

pub(crate) fn desktop_number(order: &[GUID], desktop: GUID) -> Option<u32> {
    let index = order.iter().position(|known| *known == desktop)?;
    u32::try_from(index).ok()?.checked_add(1)
}

pub(crate) const fn cloak_of(flags: u32) -> Cloak {
    if flags == 0 {
        return Cloak::Shown;
    }
    if flags & SHELL_CLOAK != 0 && flags & APP_CLOAK == 0 {
        return Cloak::ByShell;
    }
    Cloak::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: [u8; 16] = [0x78, 0x56, 0x34, 0x12, 0xBC, 0x9A, 0xF0, 0xDE, 1, 2, 3, 4, 5, 6, 7, 8];
    const SECOND: [u8; 16] = [0xAA; 16];

    #[test]
    fn guids_are_read_in_their_little_endian_registry_layout() {
        let guid = guid_from_bytes(&FIRST).unwrap();
        assert_eq!(guid, GUID::from_u128(0x1234_5678_9abc_def0_0102_0304_0506_0708));
        assert_eq!(guid_from_bytes(&FIRST[..15]), None);
    }

    #[test]
    fn the_id_list_keeps_desktop_order_and_ignores_a_torn_tail() {
        let mut bytes = FIRST.to_vec();
        bytes.extend_from_slice(&SECOND);
        bytes.extend_from_slice(&[0xFF; 5]);
        let ids = parse_desktop_ids(&bytes);
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[1], guid_from_bytes(&SECOND).unwrap());
        assert!(parse_desktop_ids(&[]).is_empty());
        assert!(parse_desktop_ids(&FIRST[..10]).is_empty());
    }

    #[test]
    fn desktops_are_numbered_from_one() {
        let ids = parse_desktop_ids(&[FIRST, SECOND].concat());
        assert_eq!(desktop_number(&ids, ids[0]), Some(1));
        assert_eq!(desktop_number(&ids, ids[1]), Some(2));
        assert_eq!(desktop_number(&ids, GUID::from_u128(7)), None);
        assert_eq!(desktop_number(&[], ids[0]), None);
    }

    #[test]
    fn only_a_shell_cloak_can_mean_another_desktop() {
        assert_eq!(cloak_of(0), Cloak::Shown);
        assert_eq!(cloak_of(0x2), Cloak::ByShell);
        assert_eq!(cloak_of(0x2 | 0x4), Cloak::ByShell);
        assert_eq!(cloak_of(0x1), Cloak::Other);
        assert_eq!(cloak_of(0x1 | 0x2), Cloak::Other);
        assert_eq!(cloak_of(0x4), Cloak::Other);
    }
}
