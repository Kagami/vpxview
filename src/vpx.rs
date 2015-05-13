//! libvpx FFI wrapper.

use std::u16;
use std::fmt;
use std::ptr;
use std::mem;
use libc::{c_int, c_uint, c_long, c_void, c_uchar};

// Safe wrapper.

#[derive(Debug)]
pub struct Error(vpx_codec_err_t);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error(ref codec_err) => write!(f, "VPx error: {:?}", codec_err)
        }
    }
}

pub struct Decoder {
    codec: Box<vpx_codec_ctx_t>,
}

impl Decoder {
    pub fn init() -> Result<Decoder, Error> {
        let mut codec = Box::new(Default::default());
        unsafe {
            let res = vpx_codec_dec_init_ver(&mut *codec,
                                             &mut vpx_codec_vp9_dx_algo,
                                             ptr::null(),
                                             0,
                                             VPX_DECODER_ABI_VERSION);
            if res == vpx_codec_err_t::VPX_CODEC_OK {
                Ok(Decoder {codec: codec})
            } else {
                Err(Error(res))
            }
        }
    }

    // FIXME(Kagami): Seems like `vpx_codec_decode` is stateful, i.e. we can't
    // run it again if we're already iterating. How can we fix it?
    pub fn decode_many(&mut self, data: &[u8]) -> Result<Frames, Error> {
        unsafe {
            let res = vpx_codec_decode(&mut *self.codec,
                                       &data[0],
                                       data.len() as c_uint,
                                       ptr::null_mut(),
                                       0);
            if res == vpx_codec_err_t::VPX_CODEC_OK {
                Ok(Frames {
                    end: false,
                    codec: &mut *self.codec,
                    iter: Box::new(ptr::null_mut()),
                })
            } else {
                Err(Error(res))
            }
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            vpx_codec_destroy(&mut *self.codec);
        }
    }
}

pub struct Frames<'c> {
    end: bool,
    codec: &'c mut vpx_codec_ctx_t,
    iter: Box<vpx_codec_iter_t>,
}

// NOTE(Kagami): We don't allow dimensions larger than u16 because:
// 1) IVF defines 2-byte dimensions
// 2) It's simpler to work with small values
// 3) Larger values are not practical anyway
const DIMENSION_MAX: c_uint = u16::MAX as c_uint;

impl<'c> Iterator for Frames<'c> {
    type Item = Image;

    fn next(&mut self) -> Option<Self::Item> {
        if self.end {
            return None;
        }
        unsafe {
            let img_data = vpx_codec_get_frame(self.codec, &mut *self.iter);
            if img_data.is_null() {
                self.end = true;
                None
            } else {
                assert!((*img_data).d_w > 0);
                assert!((*img_data).d_w <= DIMENSION_MAX);
                assert!((*img_data).d_h > 0);
                assert!((*img_data).d_h <= DIMENSION_MAX);
                Some(Image {data: img_data})
            }
        }
    }
}

pub struct Image {
    data: *mut vpx_image_t,
}

impl Image {
    pub fn get_display_width(&self) -> u16 {
        unsafe { (*self.data).d_w as u16 }
    }

    pub fn get_display_height(&self) -> u16 {
        unsafe { (*self.data).d_h as u16 }
    }

    #[inline]
    fn clamp0(val: i32) -> i32 {
        (-val >> 31) & val
    }

    #[inline]
    fn clamp255(val: i32) -> i32 {
        (((255 - val) >> 31) | val) & 255
    }

    // Branchless min/max should be faster than 2 ifs. See `YuvPixel` from
    // libyuv for details.
    #[inline]
    fn clamp(val: i32) -> u32 {
        Self::clamp255(Self::clamp0(val)) as u32
    }

    // TODO(Kagami): Use the colorspace image attribute. If it's unknown we may
    // try mpv's heuristic: use BT.709 colormatrix for dimensions larger than
    // 1279x719 (i.e. HD).
    // TODO(Kagami): SIMD!
    /// Convert YUV 8-bit pixel to RGBA8 (fully opacity) using BT.601 limited
    /// range profile. Resulting value is 4 sequential bytes representing R, G,
    /// B and A components, in that order.
    #[inline]
    fn yuv_to_rgba(y: u8, u: u8, v: u8) -> u32 {
        let (c, d, e) = (y as i32 - 16, u as i32 - 128, v as i32 - 128);
        let y1 = 298 * c + 128;
        let r = Self::clamp((y1           + 409 * e) >> 8);
        let g = Self::clamp((y1 - 100 * d - 208 * e) >> 8);
        let b = Self::clamp((y1 + 516 * d          ) >> 8);
        let a = 255;
        // TODO(Kagami): Non-LE architectures.
        a << 24 | b << 16 | g << 8 | r
    }

    /// Convert image pixels data to RGBA8 array.
    pub fn get_rgba8(&self) -> Box<[u8]> {
        unsafe {
            let d = self.data;
            // TODO(Kagami): Support other subsamplings and bit dephts.
            assert_eq!((*d).fmt, vpx_img_fmt_t::VPX_IMG_FMT_I420);
            assert_eq!((*d).bit_depth, 8);

            let y_step = (*d).stride[0] as usize;
            let u_step = (*d).stride[1] as usize;
            let v_step = (*d).stride[2] as usize;
            let mut y_offset = 0;
            let mut u_offset = 0;
            let mut v_offset = 0;
            let w = (*d).d_w as usize;
            let h = (*d).d_h as usize;
            let len = w * h;
            let mut pixels: Vec<u32> = Vec::with_capacity(len);
            pixels.set_len(len);

            for i in 0..h {
                for j in 0..w {
                    let y = *(*d).planes[0].offset((y_offset + j) as isize);
                    let u = *(*d).planes[1].offset((u_offset + j / 2) as isize);
                    let v = *(*d).planes[2].offset((v_offset + j / 2) as isize);
                    *pixels.get_unchecked_mut(i * w + j) = Self::yuv_to_rgba(y, u, v);
                }
                y_offset += y_step;
                if i % 2 != 0 {
                    u_offset += u_step;
                    v_offset += v_step;
                }
            }

            // Vec<u32> -> Box<[u8]>
            let p = pixels.as_mut_ptr() as *mut u8;
            mem::forget(pixels);
            let pixels8: Vec<u8> = Vec::from_raw_parts(p, len * 4, len * 4);
            pixels8.into_boxed_slice()
        }
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        unsafe {
            fmt::Debug::fmt(&*self.data, f)
        }
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            vpx_img_free(self.data);
        }
    }
}

// FFI mappings.

const VPX_IMAGE_ABI_VERSION: c_int = 3;
const VPX_CODEC_ABI_VERSION: c_int = 3 + VPX_IMAGE_ABI_VERSION;
const VPX_DECODER_ABI_VERSION: c_int = 3 + VPX_CODEC_ABI_VERSION;
// TODO(Kagami): Define actual struct fields instead of this hack.
const VPX_CODEC_CTX_SIZE: usize = 56;

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
#[repr(C)]
enum vpx_codec_err_t {
    VPX_CODEC_OK,
    VPX_CODEC_ERROR,
    VPX_CODEC_MEM_ERROR,
    VPX_CODEC_ABI_MISMATCH,
    VPX_CODEC_INCAPABLE,
    VPX_CODEC_UNSUP_BITSTREAM,
    VPX_CODEC_UNSUP_FEATURE,
    VPX_CODEC_CORRUPT_FRAME,
    VPX_CODEC_INVALID_PARAM,
    VPX_CODEC_LIST_END,
}

#[repr(C)]
struct vpx_codec_ctx_t([u8; VPX_CODEC_CTX_SIZE]);

impl Default for vpx_codec_ctx_t {
    fn default() -> Self {
        vpx_codec_ctx_t([0; VPX_CODEC_CTX_SIZE])
    }
}

#[derive(Debug)]
#[repr(C)]
struct vpx_codec_iface_t;

#[derive(Debug)]
#[repr(C)]
struct vpx_codec_dec_cfg_t;

#[allow(non_camel_case_types)]
type vpx_codec_flags_t = c_long;

#[allow(non_camel_case_types)]
type vpx_codec_iter_t = *mut c_void;

const VPX_IMG_FMT_PLANAR: isize = 0x100;
const VPX_IMG_FMT_UV_FLIP: isize = 0x200;
const VPX_IMG_FMT_HAS_ALPHA: isize = 0x400;
const VPX_IMG_FMT_HIGHBITDEPTH: isize = 0x800;
const VPX_IMG_FMT_I420: isize = VPX_IMG_FMT_PLANAR | 2;
const VPX_IMG_FMT_I422: isize = VPX_IMG_FMT_PLANAR | 5;
const VPX_IMG_FMT_I444: isize = VPX_IMG_FMT_PLANAR | 6;
const VPX_IMG_FMT_I440: isize = VPX_IMG_FMT_PLANAR | 7;

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
#[repr(C)]
enum vpx_img_fmt_t {
    VPX_IMG_FMT_NONE,
    VPX_IMG_FMT_RGB24,
    VPX_IMG_FMT_RGB32,
    VPX_IMG_FMT_RGB565,
    VPX_IMG_FMT_RGB555,
    VPX_IMG_FMT_UYVY,
    VPX_IMG_FMT_YUY2,
    VPX_IMG_FMT_YVYU,
    VPX_IMG_FMT_BGR24,
    VPX_IMG_FMT_RGB32_LE,
    VPX_IMG_FMT_ARGB,
    VPX_IMG_FMT_ARGB_LE,
    VPX_IMG_FMT_RGB565_LE,
    VPX_IMG_FMT_RGB555_LE,
    VPX_IMG_FMT_YV12 = VPX_IMG_FMT_PLANAR | VPX_IMG_FMT_UV_FLIP | 1,
    VPX_IMG_FMT_I420 = VPX_IMG_FMT_I420,
    VPX_IMG_FMT_VPXYV12 = VPX_IMG_FMT_PLANAR | VPX_IMG_FMT_UV_FLIP | 3,
    VPX_IMG_FMT_VPXI420 = VPX_IMG_FMT_PLANAR | 4,
    VPX_IMG_FMT_I422 = VPX_IMG_FMT_I422,
    VPX_IMG_FMT_I444 = VPX_IMG_FMT_I444,
    VPX_IMG_FMT_I440 = VPX_IMG_FMT_I440,
    VPX_IMG_FMT_444A = VPX_IMG_FMT_PLANAR | VPX_IMG_FMT_HAS_ALPHA | 6,
    VPX_IMG_FMT_I42016 = VPX_IMG_FMT_I420 | VPX_IMG_FMT_HIGHBITDEPTH,
    VPX_IMG_FMT_I42216 = VPX_IMG_FMT_I422 | VPX_IMG_FMT_HIGHBITDEPTH,
    VPX_IMG_FMT_I44416 = VPX_IMG_FMT_I444 | VPX_IMG_FMT_HIGHBITDEPTH,
    VPX_IMG_FMT_I44016 = VPX_IMG_FMT_I440 | VPX_IMG_FMT_HIGHBITDEPTH,
}

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
#[repr(C)]
enum vpx_color_space_t {
    VPX_CS_UNKNOWN = 0,
    VPX_CS_BT_601 = 1,
    VPX_CS_BT_709 = 2,
    VPX_CS_SMPTE_170 = 3,
    VPX_CS_SMPTE_240 = 4,
    VPX_CS_BT_2020 = 5,
    VPX_CS_RESERVED = 6,
    VPX_CS_SRGB = 7,
}

#[repr(C)]
struct vpx_image_t {
    fmt: vpx_img_fmt_t,
    cs: vpx_color_space_t,
    w: c_uint,
    h: c_uint,
    bit_depth: c_uint,
    d_w: c_uint,
    d_h: c_uint,
    x_chroma_shift: c_uint,
    y_chroma_shift: c_uint,
    planes: [*mut c_uchar; 4],
    stride: [c_int; 4],
    bps: c_int,
    user_priv: *mut c_void,
    img_data: *mut c_uchar,
    img_data_owner: c_int,
    self_allocd: c_int,
    fb_priv: *mut c_void,
}

impl fmt::Debug for vpx_image_t {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "\
            Image {{\n\
            \tfmt: {:?},\n\
            \tcs: {:?},\n\
            \tw: {},\n\
            \th: {},\n\
            \tbit_depth: {},\n\
            \td_w: {},\n\
            \td_h: {},\n\
            \tx_chroma_shift: {},\n\
            \ty_chroma_shift: {},\n\
            \tstride: {:?},\n\
            \tbps: {}\n\
            }}",
            self.fmt, self.cs,
            self.w, self.h,
            self.bit_depth,
            self.d_w, self.d_h,
            self.x_chroma_shift, self.y_chroma_shift,
            self.stride, self.bps,
        )
    }
}

#[link(name = "vpx")]
extern {
    static mut vpx_codec_vp9_dx_algo: vpx_codec_iface_t;

    fn vpx_codec_dec_init_ver(
        ctx: *mut vpx_codec_ctx_t,
        iface: *mut vpx_codec_iface_t,
        cfg: *const vpx_codec_dec_cfg_t,
        flags: vpx_codec_flags_t,
        ver: c_int) -> vpx_codec_err_t;

    fn vpx_codec_decode(
        ctx: *mut vpx_codec_ctx_t,
        data: *const u8,
        data_sz: c_uint,
        user_priv: *mut c_void,
        deadline: c_long) -> vpx_codec_err_t;

    fn vpx_codec_get_frame(
        ctx: *mut vpx_codec_ctx_t,
        iter: *mut vpx_codec_iter_t) -> *mut vpx_image_t;

    fn vpx_img_free(img: *mut vpx_image_t);

    fn vpx_codec_destroy(ctx: *mut vpx_codec_ctx_t) -> vpx_codec_err_t;
}
