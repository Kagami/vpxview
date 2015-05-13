use std::fmt;
use std::marker::PhantomData;
use gfx::{self, Resources, ProgramError};
use gfx::attrib::Floater;
use gfx::traits::{IntoCanvas, Factory, FactoryExt, Stream};
use gfx::shade::TextureParam;
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
use gfx_text;
use ::ivf;
use ::vpx;

#[derive(Debug)]
pub enum Error {
    GlutinCreationError(CreationError),
    GfxProgramError(ProgramError),
    GfxTextureError(TextureError),
    GfxBatchError(BatchError),
    TextError(gfx_text::Error),
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

impl From<gfx_text::Error> for Error {
    fn from(e: gfx_text::Error) -> Error { Error::TextError(e) }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let descr = match *self {
            Error::GlutinCreationError(ref err) => format!("{}", err),
            Error::GfxProgramError(ref err) => format!("{:?}", err),
            Error::GfxTextureError(ref err) => format!("{:?}", err),
            Error::GfxBatchError(ref err) => format!("{:?}", err),
            Error::TextError(ref err) => format!("{:?}", err),
        };
        f.write_str(&descr)
    }
}

gfx_vertex!( Vertex {
    a_Pos@ pos: [Floater<i8>; 2],
    a_TexCoord@ tex: [Floater<u8>; 2],
});

impl Vertex {
    fn new(pos: [i8; 2], tex: [u8; 2]) -> Self {
        Vertex {
            pos: Floater::cast2(pos),
            tex: Floater::cast2(tex),
        }
    }
}

gfx_parameters!( ShaderParams/ParamsLink {
    t_Color@ color: TextureParam<R>,
});

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

const BACKGROUND: gfx::ClearData = gfx::ClearData {
    color: [0.0, 0.0, 0.0, 1.0],
    depth: 1.0,
    stencil: 0,
};
const TEXT_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const TEXT_HEIGHT: i32 = 16;

type CanvasT = Canvas<gfxw::Output<dgl::Resources>, dgl::Device, dgl::Factory>;
type BatchT = OwnedBatch<ShaderParams<dgl::Resources>>;
type TextRendererT = gfx_text::Renderer<dgl::Resources>;

pub struct Gui {
    reader: ivf::Reader,
    decoder: vpx::Decoder,
    viewport_width: u16,
    viewport_height: u16,
    canvas: CanvasT,
    batch: BatchT,
    text: TextRendererT,
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
    let vertex_data = [
        // 1
        // |\
        // | \
        // 2--3
        Vertex::new([-1,  1], [0, 0]),
        Vertex::new([-1, -1], [0, 1]),
        Vertex::new([ 1, -1], [1, 1]),
        // 1--3
        //  \ |
        //   \|
        //    2
        Vertex::new([-1,  1], [0, 0]),
        Vertex::new([ 1, -1], [1, 1]),
        Vertex::new([ 1,  1], [1, 0]),
    ];
    let batch = {
        let mesh = canvas.factory.create_mesh(&vertex_data);
        let program = try!(canvas.factory.link_program(VERTEX_SRC, FRAGMENT_SRC));
        let texture = try!(canvas.factory.create_texture_rgba8(
            reader.get_width(),
            reader.get_height()));
        let param = ShaderParams {color: (texture, None), _r: PhantomData};
        try!(OwnedBatch::new(mesh, program, param))
    };
    let text = try!(gfx_text::new(&mut canvas.factory).build());
    Ok(Gui {
        reader: reader,
        decoder: decoder,
        viewport_width: viewport_width,
        viewport_height: viewport_height,
        canvas: canvas,
        batch: batch,
        text: text,
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
            let draw_result = self.canvas.draw(&self.batch);
            try_print!(draw_result, "Error occured while drawing the frame: {:?}");
            self.render_hud();
            self.canvas.present();
        }
    }

    /// Read next IVF frame, decode VPx frame if possible and update the
    /// texture.
    fn next_video_frame(&mut self) {
        let maybe_frame = self.reader.next();
        self.update_title();
        let ivf_frame = maybe_print!(maybe_frame, "End of file");
        match self.decoder.decode_many(&ivf_frame) {
            Ok(mut iter) => {
                let image = maybe_print!(iter.next(), "No VPx frames in this IVF frame");
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
                try_print!(update_result, "Error occured while updating texture: {:?}");
            },
            Err(err) => {
                printerr!("Cannot decode IVF frame: {}", err);
            },
        };
    }

    fn get_frame_count(&self) -> String {
        self.reader.get_frame_count().map_or_else(|| "?".to_string(), |n| n.to_string())
    }

    fn update_title(&self) {
        let title = format!("vpxview - {} - {}/{}",
                            self.reader.get_filename(),
                            self.reader.get_frame_pos(),
                            self.get_frame_count());
        self.canvas.output.window.set_title(&title);
    }

    /// Draw given lines sequentially from top to bottom.
    fn draw_lines(&mut self, start_pos: [i32; 2], lines: &[String]) {
        let (x, mut y) = (start_pos[0], start_pos[1]);
        for line in lines {
            self.text.draw(line, [x, y], TEXT_COLOR);
            y += TEXT_HEIGHT;
        }
    }

    /// Render some VPx frame details on canvas.
    fn render_hud(&mut self) {
        let lines = [
            format!("Filename: {}", self.reader.get_filename()),
            format!("Frame: {}/{}", self.reader.get_frame_pos(), self.get_frame_count()),
        ];
        self.draw_lines([10, 10], &lines);
        let draw_result = self.text.draw_end(&mut self.canvas);
        try_print!(draw_result, "Error occured why drawing the text: {:?}");
    }
}
