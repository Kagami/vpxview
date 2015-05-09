use std::fmt;
use gfx::{self, ProgramError};
use gfx::traits::{IntoCanvas, Factory, FactoryExt, Stream};
use gfx::device::tex::TextureError;
use gfx::extra::canvas::Canvas;
use gfx::batch::OwnedBatch;
use gfx::batch::Error as BatchError;
use gfx_device_gl as dgl;
use gfx_window_glutin as gfxw;
use glutin::{CreationError, WindowBuilder};
use glutin::Event::{Closed, KeyboardInput};
use glutin::ElementState::Pressed;
use glutin::VirtualKeyCode as Key;
use ::ivf;
use ::vpx;

#[derive(Debug)]
pub enum Error {
    GlutinCreationError(CreationError),
    GfxProgramError(ProgramError),
    GfxTextureError(TextureError),
    GfxBatchError(BatchError),
}

impl From<CreationError> for Error {
    fn from(e: CreationError) -> Error { Error::GlutinCreationError(e) }
}

impl From<ProgramError> for Error {
    fn from(e: ProgramError) -> Error { Error::GfxProgramError(e) }
}

impl From<TextureError> for Error {
    fn from(e: TextureError) -> Error { Error::GfxTextureError(e) }
}

impl From<BatchError> for Error {
    fn from(e: BatchError) -> Error { Error::GfxBatchError(e) }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let descr = match *self {
            Error::GlutinCreationError(ref err) => format!("{}", err),
            Error::GfxProgramError(ref err) => format!("{:?}", err),
            Error::GfxTextureError(ref err) => format!("{:?}", err),
            Error::GfxBatchError(ref err) => format!("{:?}", err),
        };
        f.write_str(&descr)
    }
}

#[vertex_format]
#[derive(Clone, Copy)]
struct Vertex {
    #[as_float]
    #[name = "a_Pos"]
    pos: [i8; 2],
    #[as_float]
    #[name = "a_TexCoord"]
    tex: [u8; 2],
}

#[shader_param]
struct ShaderParams<R: gfx::Resources> {
    #[name = "t_Color"]
    color: gfx::shade::TextureParam<R>,
    // XXX(Kagami): mute unused ToUniform import warning.
    _t: i32,
}

static VERTEX_SRC: &'static [u8] = b"
    #version 120

    attribute vec4 a_Pos;
    attribute vec2 a_TexCoord;
    varying vec2 v_TexCoord;

    void main() {
        v_TexCoord = a_TexCoord;
        gl_Position = a_Pos;
    }
";

static FRAGMENT_SRC: &'static [u8] = b"
    #version 120

    varying vec2 v_TexCoord;
    uniform sampler2D t_Color;

    void main() {
        gl_FragColor = texture2D(t_Color, v_TexCoord);
    }
";

const VERTEX_DATA: &'static [Vertex] = &[
    // 1
    // |\
    // | \
    // 2--3
    Vertex {pos: [-1,  1], tex: [0, 0]},
    Vertex {pos: [-1, -1], tex: [0, 1]},
    Vertex {pos: [ 1, -1], tex: [1, 1]},
    // 1--3
    //  \ |
    //   \|
    //    2
    Vertex {pos: [-1,  1], tex: [0, 0]},
    Vertex {pos: [ 1, -1], tex: [1, 1]},
    Vertex {pos: [ 1,  1], tex: [1, 0]},
];

const BACKGROUND: gfx::ClearData = gfx::ClearData {
    color: [0.0, 0.0, 0.0, 1.0],
    depth: 1.0,
    stencil: 0,
};

type CanvasT = Canvas<gfxw::Output<dgl::Resources>, dgl::Device, dgl::Factory>;
type BatchT = OwnedBatch<ShaderParams<dgl::Resources>>;

pub struct Gui {
    reader: ivf::Reader,
    decoder: vpx::Decoder,
    viewport_width: u16,
    viewport_height: u16,
    canvas: CanvasT,
    batch: BatchT,
}

pub fn init(reader: ivf::Reader, decoder: vpx::Decoder) -> Result<Gui, Error> {
    let viewport_width = reader.get_width();
    let viewport_height = reader.get_height();
    let mut canvas = {
        // TODO(Kagami): Fullscreen.
        let window = try!(WindowBuilder::new()
            .with_dimensions(viewport_width as u32, viewport_height as u32)
            // Use simple initial title to allow to match the window in tiling
            // window managers.
            .with_title(format!("vpxview"))
            .build());
        gfxw::init(window).into_canvas()
    };
    let batch = {
        let mesh = canvas.factory.create_mesh(VERTEX_DATA);
        let program = try!(canvas.factory.link_program(VERTEX_SRC, FRAGMENT_SRC));
        let texture = try!(canvas.factory.create_texture_rgba8(
            reader.get_width(),
            reader.get_height()));
        let param = ShaderParams {color: (texture, None), _t: 0};
        try!(OwnedBatch::new(mesh, program, param))
    };
    Ok(Gui {
        reader: reader,
        decoder: decoder,
        viewport_width: viewport_width,
        viewport_height: viewport_height,
        canvas: canvas,
        batch: batch,
    })
}

impl Gui {
    pub fn run(&mut self) {
        self.next_video_frame();
        loop {
            // Skip all pending events except the first because in some cases frame
            // decoding may take too long so interface will be brozen because of
            // big events queue.
            let maybe_event = {
                let mut iter = self.canvas.output.window.poll_events();
                let ev = iter.next();
                iter.count();  // Consume entire iterator
                ev
            };
            match maybe_event {
                Some(Closed) => break,
                Some(KeyboardInput(Pressed, _, Some(Key::Escape))) => break,
                Some(KeyboardInput(Pressed, _, Some(Key::Q))) => break,
                Some(KeyboardInput(Pressed, _, Some(Key::Left))) => {
                    // TODO(Kagami).
                },
                Some(KeyboardInput(Pressed, _, Some(Key::Right))) => {
                    self.next_video_frame();
                },
                _ => {},
            }
            self.canvas.clear(BACKGROUND);
            match self.canvas.draw(&self.batch) {
                Err(err) => printerr!("Error occured while drawing the frame: {:?}", err),
                Ok(_) => {},
            }
            self.canvas.present();
        }
    }

    /// Read next IVF frame, decode VPx frame if possible and update the
    /// texture.
    fn next_video_frame(&mut self) {
        let maybe_frame = self.reader.next();
        self.update_title();
        let ivf_frame = try_print!(maybe_frame, "End of file");
        match self.decoder.decode_many(&ivf_frame) {
            Ok(mut iter) => {
                let image = try_print!(iter.next(), "No VPx frames in this IVF frame");
                // TODO(Kagami): IVF frame may consist of several VPx frames, we
                // correctly display only 1 IVF <-> 1 VPx case as for now.
                let remaining = iter.count();
                if remaining != 0 {
                    printerr!("Skipping {} other VPx frames", remaining);
                }
                // TODO(Kagami): Dimensions of decoded VPx image can vary from
                // frame to frame, we can adjust texture size accordingly.
                assert_eq!(image.get_display_width(), self.viewport_width);
                assert_eq!(image.get_display_height(), self.viewport_height);
                let texture = &self.batch.param.color.0;
                let update_result = self.canvas.factory.update_texture_raw(
                    texture,
                    &texture.get_info().to_image_info(),
                    &image.get_rgba8(),
                    None);
                match update_result {
                    Err(err) =>
                        printerr!("Error occured while updating texture: {:?}", err),
                    Ok(_) =>
                        {},
                }
            },
            Err(err) => {
                printerr!("Cannot decode IVF frame: {}", err);
            },
        };
    }

    fn update_title(&self) {
        let reader = &self.reader;
        let frame_count = reader.get_frame_count()
                                .map_or_else(|| "?".to_string(), |v| v.to_string());
        let title = format!("vpxview - {} - {}/{}",
                            reader.get_filename(),
                            reader.get_frame_pos(),
                            frame_count);
        println!("Frame {}/{}", reader.get_frame_pos(), frame_count);
        self.canvas.output.window.set_title(&title);
    }
}
