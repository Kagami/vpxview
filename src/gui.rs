use gfx;
use gfx::traits::*;
use gfx::device::handle::Texture;
use gfx::extra::canvas::Canvas;
use gfx_device_gl as dgl;
use gfx_window_glutin::{self, Wrap};
use glutin::{CreationError, Window, WindowBuilder, Event, VirtualKeyCode, ElementState};
use ::ivf;
use ::vpx;

pub type Error = CreationError;

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

// TODO(Kagami): Process all errors.
// TODO(Kagami): Fullscreen.
pub fn run(reader: &mut ivf::Reader, decoder: &mut vpx::Decoder) -> Result<(), Error> {
    let mut canvas = {
        let window = try!(WindowBuilder::new()
            .with_dimensions(reader.get_width() as u32, reader.get_height() as u32)
            // Use simple initial title to allow to match the window in tiling
            // window managers.
            .with_title(format!("vpxview"))
            .build());
        gfx_window_glutin::init(window).into_canvas()
    };
    let batch = {
        let mesh = canvas.factory.create_mesh(VERTEX_DATA);
        let program = canvas.factory.link_program(VERTEX_SRC, FRAGMENT_SRC).unwrap();
        let texture = canvas.factory.create_texture_rgba8(
            reader.get_width(),
            reader.get_height()).unwrap();
        let param = ShaderParams {color: (texture, None)};
        gfx::batch::OwnedBatch::new(mesh, program, param).unwrap()
    };
    let texture = &batch.param.color.0;

    show_frame(reader, decoder, &mut canvas, texture);
    'main: loop {
        // We need to pass mutable reference to canvas to `show_frame` so we
        // need to grab all events first.
        let events: Vec<Event> = canvas.output.window.poll_events().collect();
        for event in events {
            match event {
                Event::Closed => break 'main,
                Event::KeyboardInput(_, _, Some(VirtualKeyCode::Escape)) => break 'main,
                Event::KeyboardInput(_, _, Some(VirtualKeyCode::Q)) => break 'main,
                Event::KeyboardInput(ElementState::Pressed, _, Some(VirtualKeyCode::Left)) => {
                    // TODO(Kagami).
                },
                Event::KeyboardInput(ElementState::Pressed, _, Some(VirtualKeyCode::Right)) => {
                    show_frame(reader, decoder, &mut canvas, texture);
                },
                _ => {},
            }
        }
        canvas.clear(gfx::ClearData {color: [0.0, 0.0, 0.0, 1.0], depth: 1.0, stencil: 0});
        canvas.draw(&batch).unwrap();
        canvas.present();
    }
    Ok(())
}

type CanvasT = Canvas<Wrap<dgl::Resources>, dgl::Device, dgl::Factory>;
type TextureT = Texture<dgl::Resources>;

fn show_frame(reader: &mut ivf::Reader,
              decoder: &mut vpx::Decoder,
              canvas: &mut CanvasT,
              texture: &TextureT) {
    let maybe_frame = reader.next();
    update_title(reader, &canvas.output.window);
    if maybe_frame.is_none() {
        return println!("End of file");
    }
    let ivf_frame = maybe_frame.unwrap();
    match decoder.decode_many(&ivf_frame) {
        Ok(mut iter) => {
            let image = iter.next().unwrap();
            // TODO(Kagami): IVF frame may consist of several VPx frames.
            assert_eq!(iter.count(), 0);
            // TODO(Kagami): Dimensions of decoded VPx image can vary from
            // frame to frame, we can adjust texture size accordingly.
            assert_eq!(image.get_display_width(), texture.get_info().width);
            assert_eq!(image.get_display_height(), texture.get_info().height);
            canvas.factory.update_texture_raw(
                texture,
                &texture.get_info().to_image_info(),
                &image.get_rgba8(),
                None).unwrap();
        },
        Err(err) => {
            printerr!("Cannot decode IVF frame: {}", err);
        },
    };
}

fn update_title(reader: &ivf::Reader, window: &Window) {
    let frame_count = reader.get_frame_count()
                            .map_or_else(|| "?".to_string(), |v| v.to_string());
    let title = format!("vpxview - {} - {}/{}",
                        reader.get_filename(),
                        reader.get_frame_pos(),
                        frame_count);
    println!("Frame {}/{}", reader.get_frame_pos(), frame_count);
    window.set_title(&title);
}
