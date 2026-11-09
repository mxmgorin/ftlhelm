//! A pad's Linux event node, read for the log: its ids, its axes with their ranges, and the
//! axes that moved from where they rested when the node opened. SIL numbers its device table
//! by event node, so the node of SIL's device is tried first; its name must match SIL's, else
//! the other nodes are searched for that name. The node is only queried, never grabbed or
//! read for events.

use std::fs::File;
use std::os::fd::AsRawFd;

use crate::core::util::nul_terminated;

/// The event nodes SIL scans.
const EVENT_NODES: i32 = 32;
const EV_ABS: u64 = 0x03;
const ABS_CNT: usize = 0x40;
const NAME_LEN: usize = 256;

/// The asm-generic ioctl encoding, shared by x86-64 and arm64.
const IOC_READ: u64 = 2;
const IOC_DIR_SHIFT: u64 = 30;
const IOC_SIZE_SHIFT: u64 = 16;
const IOC_TYPE_SHIFT: u64 = 8;
const EVDEV_IOC_TYPE: u64 = b'E' as u64;
const EVIOCGID_NR: u64 = 0x02;
const EVIOCGNAME_NR: u64 = 0x06;
const EVIOCGBIT_NR: u64 = 0x20;
const EVIOCGABS_NR: u64 = 0x40;

/// struct input_id: bus, vendor, product, version.
type InputId = [u16; 4];
/// struct input_absinfo: value, minimum, maximum, fuzz, flat, resolution.
type AbsInfo = [i32; 6];
const ABS_VALUE: usize = 0;
const ABS_MIN: usize = 1;
const ABS_MAX: usize = 2;
/// An axis counts as moved past its range over MOVED_DIVISOR from its rest.
const MOVED_DIVISOR: i64 = 4;

/// An evdev read request for `size` bytes, number `nr`.
const fn ior(nr: u64, size: usize) -> u64 {
    IOC_READ << IOC_DIR_SHIFT | (size as u64) << IOC_SIZE_SHIFT | EVDEV_IOC_TYPE << IOC_TYPE_SHIFT | nr
}

/// Reads `out` with ioctl `request`; whether it succeeded.
fn query<T>(file: &File, request: u64, out: &mut T) -> bool {
    // SAFETY: every request here writes at most size_of::<T>() bytes, the size it encodes.
    unsafe { libc::ioctl(file.as_raw_fd(), request as _, out as *mut T) >= 0 }
}

fn node_name(file: &File) -> Option<String> {
    let mut buf = [0u8; NAME_LEN];
    query(file, ior(EVIOCGNAME_NR, NAME_LEN), &mut buf).then(|| nul_terminated(&buf))
}

fn abs_info(file: &File, code: u16) -> Option<AbsInfo> {
    let mut info: AbsInfo = [0; 6];
    query(file, ior(EVIOCGABS_NR + code as u64, size_of::<AbsInfo>()), &mut info).then_some(info)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    pub code: u16,
    pub min: i32,
    pub max: i32,
}

pub struct EventNode {
    file: File,
    pub node: i32,
    pub id: InputId,
    pub axes: Vec<Axis>,
    /// The axes' values when the node opened.
    rest: Vec<i32>,
}

impl EventNode {
    /// Opens the event node of SIL's device `event_dev` named `name`, else the first other
    /// node by that name; None when none opens.
    pub fn open(event_dev: i32, name: &str) -> Option<EventNode> {
        // Tests leave the machine's own input devices alone.
        if cfg!(test) {
            return None;
        }
        let others = (0..EVENT_NODES).filter(|&n| n != event_dev);
        std::iter::once(event_dev).chain(others).find_map(|node| {
            let file = File::open(format!("/dev/input/event{node}")).ok()?;
            (node_name(&file).as_deref() == Some(name)).then(|| EventNode::read(file, node))
        })
    }

    fn read(file: File, node: i32) -> EventNode {
        let mut id: InputId = [0; 4];
        query(&file, ior(EVIOCGID_NR, size_of::<InputId>()), &mut id);
        let mut bits = [0u8; ABS_CNT / 8];
        if !query(&file, ior(EVIOCGBIT_NR + EV_ABS, bits.len()), &mut bits) {
            bits = [0; ABS_CNT / 8];
        }
        let axes = (0..ABS_CNT as u16)
            .filter(|&c| bits[c as usize / 8] >> (c % 8) & 1 != 0)
            .filter_map(|code| abs_info(&file, code).map(|i| Axis { code, min: i[ABS_MIN], max: i[ABS_MAX] }))
            .collect();
        let mut node = EventNode { file, node, id, axes, rest: Vec::new() };
        node.rest = node.values();
        node
    }

    /// The axes' values, in `axes` order; an unreadable one reads as its minimum.
    fn values(&self) -> Vec<i32> {
        self.axes.iter().map(|a| abs_info(&self.file, a.code).map_or(a.min, |i| i[ABS_VALUE])).collect()
    }

    /// The axes away from their rest now, with their values.
    pub fn moved(&self) -> Vec<(u16, i32)> {
        moved(&self.axes, &self.rest, &self.values())
    }
}

/// The axes whose value is away from their rest, with their values.
fn moved(axes: &[Axis], rest: &[i32], values: &[i32]) -> Vec<(u16, i32)> {
    let away = |a: &Axis, r: i32, v: i32| (v as i64 - r as i64).abs() > (a.max as i64 - a.min as i64) / MOVED_DIVISOR;
    (axes.iter().zip(rest).zip(values)).filter(|&((a, &r), &v)| away(a, r, v)).map(|((a, _), &v)| (a.code, v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_match_the_kernel_macros() {
        assert_eq!(ior(EVIOCGID_NR, size_of::<InputId>()), 0x8008_4502);
        assert_eq!(ior(EVIOCGNAME_NR, NAME_LEN), 0x8100_4506);
        assert_eq!(ior(EVIOCGBIT_NR + EV_ABS, ABS_CNT / 8), 0x8008_4523);
        assert_eq!(ior(EVIOCGABS_NR, size_of::<AbsInfo>()), 0x8018_4540);
    }

    #[test]
    fn an_axis_counts_as_moved_past_a_quarter_of_its_range_from_its_rest() {
        let axes = [
            Axis { code: 0, min: -32768, max: 32767 },
            Axis { code: 1, min: 0, max: 4095 },
            Axis { code: 2, min: 0, max: 255 },
        ];
        let rest = [0, 2048, 0];
        assert_eq!(moved(&axes, &rest, &rest), [], "a trigger resting at one end");
        assert_eq!(moved(&axes, &rest, &[-20000, 3500, 255]), [(0, -20000), (1, 3500), (2, 255)]);
        assert_eq!(moved(&axes, &rest, &[-8000, 1100, 60]), [], "within a quarter range");
    }
}
