//! IVF container parser.
//! Reference: <http://wiki.multimedia.cx/index.php?title=IVF>.

use std::fmt;
use std::io;
use std::io::Read;
use std::fs::File;
use ::common;

const DKIF: [u8; 4] = [68, 75, 73, 70];
const VP9_FOURCC: u32 = 0x30395056;

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    // TODO(Kagami): Better granularity of parse errors.
    ParseError,
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error { Error::IoError(err) }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let descr = match *self {
            Error::IoError(ref err) => format!("IO error: {}", err),
            Error::ParseError => format!("Parse error"),
        };
        f.write_str(&descr)
    }
}

// TODO(Kagami): Better BufReader.
pub fn read_bytes(breader: &mut io::BufReader<File>,
                  count: usize) -> Result<Box<[u8]>, Error> {
    let mut buf = common::alloc(count);
    let mut collected = 0;
    while collected < count {
        let chunk_size = try!(breader.read(&mut buf[collected..]));
        if chunk_size == 0 {
            return Err(Error::ParseError);
        }
        collected += chunk_size;
    }
    Ok(buf)
}

pub struct Reader {
    breader: io::BufReader<File>,
    filename: String,
    #[allow(dead_code)]
    fourcc: u32,
    width: u16,
    height: u16,
    /// Frame position we are currently viewing file at.
    /// Set to 0 after file header was read.
    frame_pos: usize,
    frame_count: Option<usize>,
}

impl Reader {
    // It's a shame Rust doesn't have const struct fields...
    pub fn get_filename(&self) -> &str { &self.filename }
    #[allow(dead_code)]
    pub fn get_fourcc(&self) -> u32 { self.fourcc }
    pub fn get_width(&self) -> u16 { self.width }
    pub fn get_height(&self) -> u16 { self.height }
    pub fn get_frame_pos(&self) -> usize { self.frame_pos }
    pub fn get_frame_count(&self) -> Option<usize> { self.frame_count }

    pub fn open(filename: String) -> Result<Reader, Error> {
        let fh = try!(File::open(&filename));
        let mut breader = io::BufReader::new(fh);
        let header = try!(read_bytes(&mut breader, 32));
        // Parse and check only few header fields. It's better if we can view a
        // quite corruped files too.
        if &header[..4] != DKIF {
            return Err(Error::ParseError);
        }
        let fourcc = common::get_le32(&header[8..]);
        // TODO(Kagami): Support for VP8.
        if fourcc != VP9_FOURCC {
            return Err(Error::ParseError);
        }
        let width = common::get_le16(&header[12..]);
        let height = common::get_le16(&header[14..]);
        if width == 0 || height == 0 {
            return Err(Error::ParseError);
        }
        Ok(Reader {
            breader: breader,
            filename: filename,
            fourcc: fourcc,
            width: width,
            height: height,
            frame_pos: 0,
            frame_count: None,
        })
    }
}

impl Iterator for Reader {
    type Item = Box<[u8]>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.frame_count {
            Some(count) if self.frame_pos >= count => return None,
            _ => {},
        }
        match read_bytes(&mut self.breader, 12) {
            Ok(fheader) => {
                let fsize = common::get_le32(&fheader[..]) as usize;
                match read_bytes(&mut self.breader, fsize) {
                    Ok(frame) => {
                        self.frame_pos += 1;
                        Some(frame)
                    },
                    Err(_) => {
                        // No more frames.
                        // TODO: Panic on non-EOF errors.
                        self.frame_count = Some(self.frame_pos);
                        None
                    },
                }
            },
            Err(_) => {
                // No more frames.
                // NOTE(Kagami): IVF header has *number of frames in file*
                // property per spec, but ffmpeg sets 0 to that field for some
                // reason. So we just read the file until we reached the end.
                // TODO: Panic on non-EOF errors.
                self.frame_count = Some(self.frame_pos);
                None
            },
        }
    }
}

// TODO(Kagami): prev().
