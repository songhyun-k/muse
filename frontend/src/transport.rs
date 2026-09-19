use crate::{
    generated::MAX_MESSAGE_BYTES,
    generated::{Event, Request},
    wire,
};
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Bridge {
    pub abi_version: u32,
    pub context: *mut c_void,
    pub submit: Option<unsafe extern "C" fn(*mut c_void, *const u8, usize) -> i32>,
    pub receive: Option<unsafe extern "C" fn(*mut c_void, *mut u8, usize) -> i32>,
}

#[derive(Debug, PartialEq)]
pub enum Admission {
    Accepted,
    Busy,
    Closed,
}

pub struct Channel {
    bridge: Bridge,
    buffer: Vec<u8>,
    pub closed: bool,
}

impl Channel {
    /// # Safety
    /// The host must keep context and callbacks valid until this channel is dropped.
    /// Callbacks synchronously copy buffers and must not unwind or retain pointers.
    pub unsafe fn new(bridge: Bridge) -> Result<Self, String> {
        if bridge.abi_version != 1
            || bridge.context.is_null()
            || bridge.submit.is_none()
            || bridge.receive.is_none()
        {
            return Err("Invalid transport ABI".into());
        }
        Ok(Self {
            bridge,
            buffer: vec![0; MAX_MESSAGE_BYTES],
            closed: false,
        })
    }

    pub fn send(&self, request: &Request) -> Result<Admission, String> {
        let data = wire::encode_request(request)?;
        // SAFETY: new checks callbacks; data remains alive for the synchronous call.
        let result = unsafe {
            (self.bridge.submit.unwrap())(self.bridge.context, data.as_ptr(), data.len())
        };
        match result {
            0 => Ok(Admission::Accepted),
            1 => Ok(Admission::Busy),
            2 => Ok(Admission::Closed),
            _ => Err("Transport rejected message".into()),
        }
    }

    pub fn receive(&mut self) -> Result<Option<Event>, String> {
        // SAFETY: the provided buffer is writable for its full stated capacity.
        let count = unsafe {
            (self.bridge.receive.unwrap())(
                self.bridge.context,
                self.buffer.as_mut_ptr(),
                self.buffer.len(),
            )
        };
        match count {
            -1 => {
                self.closed = true;
                Ok(None)
            }
            0 => Ok(None),
            n if n > 0 && (n as usize) <= self.buffer.len() => {
                wire::decode_event(&self.buffer[..n as usize]).map(Some)
            }
            _ => Err("Transport returned an invalid message size".into()),
        }
    }
}
