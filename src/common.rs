//! Common routines.

use std::fmt;
use ::ivf;
use ::gui;
use ::vpx;

/// Universal error type across all submodules.
#[derive(Debug)]
pub enum Error {
    IvfError(ivf::Error),
    GuiError(gui::Error),
    VpxError(vpx::Error),
}

// Boilerplate :/
// At first we need to wrap error into common error type to make the `try!`
// work, then we need to wrap it out before displaying.
impl From<ivf::Error> for Error { fn from(e: ivf::Error) -> Error { Error::IvfError(e) } }
impl From<gui::Error> for Error { fn from(e: gui::Error) -> Error { Error::GuiError(e) } }
impl From<vpx::Error> for Error { fn from(e: vpx::Error) -> Error { Error::VpxError(e) } }

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let descr = match *self {
            Error::IvfError(ref err) => format!("{}", err),
            Error::GuiError(ref err) => format!("{}", err),
            Error::VpxError(ref err) => format!("{}", err),
        };
        f.write_str(&descr)
    }
}

pub fn alloc<T>(size: usize) -> Box<[T]> {
    // Seems like there is no easier safe way (i.e. without losing auto memory
    // management) to allocate memory area.
    let mut buf = Vec::with_capacity(size);
    unsafe { buf.set_len(size); }
    buf.into_boxed_slice()
}

pub fn get_le32(buf: &[u8]) -> u32 {
    let mut val = (buf[3] as u32) << 24;
    val |= (buf[2] as u32) << 16;
    val |= (buf[1] as u32) << 8;
    val |= buf[0] as u32;
    val
}

pub fn get_le16(buf: &[u8]) -> u16 {
    let mut val = (buf[1] as u16) << 8;
    val |= buf[0] as u16;
    val
}

macro_rules! printerr {
    ($fmt:expr) =>
        (::std::io::Write
            ::write_fmt(&mut ::std::io::stderr(), format_args!(concat!($fmt, "\n")))
            .unwrap());
    ($fmt:expr, $($arg:tt)*) =>
        (::std::io::Write
            ::write_fmt(&mut ::std::io::stderr(), format_args!(concat!($fmt, "\n"), $($arg)*))
            .unwrap());
}

macro_rules! try_print {
    ($expr:expr, $fmt:expr) => (match $expr {
        ::std::option::Option::Some(val) => val,
        _ => return $crate::printerr!($fmt),
    })
}
