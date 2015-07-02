extern crate libc;
extern crate clock_ticks;
extern crate errno;
extern crate glutin;
#[macro_use]
extern crate glium;

mod ctrl;
mod pty;

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
}

implement_vertex!(Vertex, position, color);

fn main() {
    use clock_ticks;
    use libc;
    use std::io;
    use std::process;
    use std::thread;
    use glium::DisplayBuild;
    use glium::index;
    use glium::Surface;
    
    let (mut m, s) = pty::open().unwrap();
    
    match unsafe { libc::fork() } {
        -1   => panic!(io::Error::last_os_error()),
         0   => pty::run_sh(m, s),
         pid => {
            print!("master, child pid: {}\n", pid);
            
            m.set_noblock();
            
            
            print!("Starting");
            
            let mut p   = ctrl::new_parser(io::BufReader::with_capacity(100, m));
            let display = glutin::WindowBuilder::new().with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (3, 3))).build_glium().unwrap();

let vertex_buffer = glium::VertexBuffer::new(&display, vec![
    Vertex { position: [-0.5, -0.5], color: [0.0, 1.0, 0.0] },
    Vertex { position: [ 0.0,  0.5], color: [0.0, 0.0, 1.0] },
    Vertex { position: [ 0.5, -0.5], color: [1.0, 0.0, 0.0] },
]);
let indices = index::NoIndices(index::PrimitiveType::TrianglesList);
let program = glium::Program::from_source(&display,
    // vertex shader
    "   #version 410

        uniform mat4 matrix;

        in vec2 position;
        in vec3 color;

        out vec3 v_color;

        void main() {
            gl_Position = vec4(position, 0.0, 1.0) * matrix;
            v_color = color;
        }
    ",

    // fragment shader
    "   #version 410
        in vec3 v_color;
        out vec4 FragColor;

        void main() {
            FragColor = vec4(v_color, 1.0);
        }
    ",

    // optional geometry shader
    None
).unwrap();
let uniforms = uniform! {
    matrix: [
        [ 1.0, 0.0, 0.0, 0.0 ],
        [ 0.0, 1.0, 0.0, 0.0 ],
        [ 0.0, 0.0, 1.0, 0.0 ],
        [ 0.0, 0.0, 0.0, 1.0 ]
    ]
};
            
            display.get_window().map(|w| w.set_title("rust_term"));
            
            unsafe { display.get_window().map(|w| w.make_current()); };
            
            let mut accumulator    = 0;
            let mut previous_clock = clock_ticks::precise_time_ns();
            let mut has_data       = false;
            let mut prev_win_size  = display.get_window().and_then(|w| w.get_inner_size());
            
            loop {
                let now = clock_ticks::precise_time_ns();
                accumulator += now - previous_clock;
                previous_clock = now;
                const FIXED_TIME_STAMP: u64 = 16666667;
                
                while accumulator >= FIXED_TIME_STAMP {
                    accumulator -= FIXED_TIME_STAMP;
                    
                    loop {
                        match p.next() {
                            Some(c) => {
                                has_data = true;
                                
                                match c {
                                    ctrl::Seq::SetWindowTitle(ref title) => {
                                        display.get_window().map(|w| w.set_title(title));
                                    },
                                    c                                    => println!("> {:?}", c)
                                }
                            },
                            None    => break
                        }
                    }
                    
                    if has_data {
                    
let mut target = display.draw();
target.clear_color(0.0, 0.0, 0.0, 0.0);  // filling the output with the black color
target.draw(&vertex_buffer, &indices, &program, &uniforms,
            &std::default::Default::default()).unwrap();
target.finish();
                    // display.get_window().map(|w| w.swap_buffers());
                    }
                    
                    for i in display.poll_events() {
                        match i {
                            glutin::Event::Closed => process::exit(0),
                            _                     => println!("w {:?}", i)
                        }
                    }
                    
                    let win_size  = display.get_window().and_then(|w| w.get_inner_size());
                    
                    if win_size != prev_win_size {
                        prev_win_size = win_size;
                        has_data      = true;
                    }
                    else {
                        has_data = false;
                    }
                }
                
                thread::sleep_ms(((FIXED_TIME_STAMP - accumulator) / 1000000) as u32);
            }
         }
    }
}
