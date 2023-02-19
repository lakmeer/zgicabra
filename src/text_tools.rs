
//
// Other Formatters
//

fn minigauge (x: f32) -> String {
    let y = (x * 8.0) as u8;
    match y {
        0 => " ".to_string(),
        1 => "▁".to_string(),
        2 => "▂".to_string(),
        3 => "▃".to_string(),
        4 => "▄".to_string(),
        5 => "▅".to_string(),
        6 => "▆".to_string(),
        7 => "▇".to_string(),
        8 => "█".to_string(),
        _ => "×".to_string()
    }
}

fn minidir (d: Direction) -> String {
    match d {
        Direction::None      => "(·)".to_string(),
        Direction::Up        => "(↑)".to_string(),
        Direction::UpRight   => "(↗)".to_string(),
        Direction::Right     => "(→)".to_string(),
        Direction::DownRight => "(↘)".to_string(),
        Direction::Down      => "(↓)".to_string(),
        Direction::DownLeft  => "(↙)".to_string(),
        Direction::Left      => "(←)".to_string(),
        Direction::UpLeft    => "(↖)".to_string(),
    }
}

fn ministick (stick: Joystick) -> String {
    if stick.clicked {
        "(⬤)".to_string()
    } else {
        minidir(stick.octant)
    }
}

fn minibar (x: f32, n: u8) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < n {
        if x > (i as f32) / n as f32 {
            s.push_str("█");
        } else {
            s.push_str("░");
        }
        i += 1;
    }
    s
}

fn minimask (x: u32) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < 8 {
        if x & (1 << i) != 0 {
            s.push_str("•");
        } else {
            s.push_str("◦");
        }
        i += 1;
    }
    s
}

pub fn format_frame (maybe_frame: Option<&ControllerFrame>) -> String {
    match maybe_frame {
        None => "No Data".to_string(),
        Some(frame) => {
            format!("{}/{}:|{}|:{}:[ {: >6.2} {: >6.2} {: >6.2} | {: >5.3} {: >5.2} {: >5.2} {: >5.2} ]", 
                    minibar(frame.joystick_x, 4),
                    minibar(frame.joystick_y, 4),
                    minimask(frame.buttons),
                    minigauge(frame.trigger),
                    frame.pos[0], frame.pos[1], frame.pos[2],
                    frame.rot_quat[0], frame.rot_quat[1],
                    frame.rot_quat[2], frame.rot_quat[3]
                   )
        }
    }
}

pub fn format_wand (wand: &Wand) -> String {
    format!("{}▕{}▏]{}[ -{}-{}{}{}{}-",
            ministick(wand.stick),
            minigauge(wand.trigger),
            if wand.bumper     { "█" } else { "░" },
            if wand.home       { "H" } else { "░" },
            if wand.buttons[0] { "1" } else { "░" },
            if wand.buttons[1] { "2" } else { "░" },
            if wand.buttons[2] { "3" } else { "░" },
            if wand.buttons[3] { "4" } else { "░" },
            )
}


pub fn draw_octants(turtle: &mut drawille::Turtle, x: f32, y: f32, theta: f32, oct_edge: f32, my_color: drawille::PixelColor, octant: Direction) {

    let oct_rad = oct_edge * 1.30656; // oct_edge * 0.5 / (PI / 8.0).sin();

    turtle.up();
    turtle.teleport(x, y);
    turtle.left(22.5);
    turtle.color(my_color);
    turtle.down();

    /*
    for i in 0..8 {
        turtle.forward(oct_rad);
        turtle.right(22.5 + 90.0);
        turtle.forward(oct_edge);                                                      
        turtle.right(22.5 + 90.0);
        turtle.up();
        turtle.forward(oct_rad);
        turtle.down();
        turtle.right(180.0);
    }
    */

    turtle.left(theta * 180.0 / PI);

    for i in 0..8 {
        turtle.forward(oct_rad);
        turtle.up();
        turtle.back(oct_rad);                                                      
        turtle.right(45.0);
        turtle.down();
    }


    // Joystick Octants: Selected Octant

    turtle.color(drawille::PixelColor::White);
    let facing = 225.0 + 45.0 * octant as i32 as f32;

    if octant != Direction::None {
        turtle.right(facing);

        for i in 0..40 {
            let len = oct_rad * (0.83 + 0.15 * rand::thread_rng().sample::<f32,_>(StandardNormal));
            if rand::random::<f32>() < 0.2 { turtle.down(); } else { turtle.up(); }
            turtle.forward(len);
            turtle.back(len);
            turtle.right(45.0/40.0);
        }

        turtle.left(45.0);
        turtle.left(facing);
    }


    // Joystick Octants: Border touchup

    /*
    turtle.up();
    turtle.forward(oct_rad);
    turtle.right(90.0 + 22.5);
    turtle.color(my_color);
    turtle.down();

    for i in 0..8 {
        turtle.forward(oct_edge);
        turtle.right(45.0);
    }
    */

    turtle.right(theta * 180.0 / PI);
    turtle.left(-22.5);
}


pub fn drawille_button (turtle: &mut drawille::Turtle, x: f32, y: f32, pressed: bool, skew: f32, my_color: drawille::PixelColor) {
    turtle.up();
    turtle.teleport(x, y);
    turtle.color(my_color);
    turtle.down();

    let h = 9.0;

    for i in 0..2 {
        turtle.forward(h);
        turtle.right(90.0);
        turtle.forward(h);
        turtle.right(90.0);
    }

    if pressed {
        turtle.up();
        turtle.teleport(x + 1.0, y - 1.0);
        turtle.color(drawille::PixelColor::White);
        turtle.down();

        scribble(turtle, h-2.0, h-2.0, true);
    }
}

fn draw_wand (wand: Wand, x: u16, y: u16) {
    const WIDTH  : f32 = 58.0;
    const HEIGHT : f32 = 95.0;

    let mut turtle = drawille::Turtle::new(0.0, 0.0);

    let my_color = match wand.hand {
        Hand::Left => drawille::PixelColor::Blue,
        Hand::Right => drawille::PixelColor::Green,
        Hand::Unknown => drawille::PixelColor::Red
    };

    let my_bright = match wand.hand {
        Hand::Left => drawille::PixelColor::BrightBlue,
        Hand::Right => drawille::PixelColor::BrightGreen,
        Hand::Unknown => drawille::PixelColor::BrightRed
    };



    //
    // Canvas outputs
    //

    // Border

    /*
    turtle.up();
    turtle.teleport(1.0, 1.0);
    turtle.down();

    for i in 0..2 {
        turtle.forward(WIDTH);
        turtle.right(90.0);
        turtle.forward(HEIGHT);
        turtle.right(90.0);
    }
    */


    // Bumper Beam

    turtle.up();
    turtle.teleport(7.0, 9.0);
    turtle.color(my_color);
    turtle.left(90.0);
    turtle.down();

    draw_trigger(&mut turtle, 45.0, 5.0, my_color, wand.bumper as i8 as f32);


    // Trigger Beam

    turtle.up();
    turtle.teleport(7.0, 19.0);

    draw_trigger(&mut turtle, 45.0, 9.0, my_color, wand.trigger);


    // Joystick Octants

    draw_octants(&mut turtle, 29.5, 54.0, wand.rot[2], 19.0, my_color, wand.stick.octant);


    // Buttons

    let button_y = 92.0;

    match wand.hand {
        Hand::Left => {
            drawille_button(&mut turtle,  8.0, button_y, wand.buttons[3], 45.0, my_color);
            drawille_button(&mut turtle, 20.0, button_y, wand.buttons[1],  0.0, my_color);
            drawille_button(&mut turtle, 32.0, button_y, wand.buttons[0],  0.0, my_color);
            drawille_button(&mut turtle, 44.0, button_y, wand.buttons[2],-45.0, my_color);
        },
        Hand::Right => {
            drawille_button(&mut turtle,  8.0, button_y, wand.buttons[2], 45.0, my_color);
            drawille_button(&mut turtle, 20.0, button_y, wand.buttons[0],  0.0, my_color);
            drawille_button(&mut turtle, 32.0, button_y, wand.buttons[1],  0.0, my_color);
            drawille_button(&mut turtle, 44.0, button_y, wand.buttons[3],-45.0, my_color);
        },
        _ => {}
    }

    turtle.up();
    turtle.teleport(18.0, 62.0);
    turtle.right(180.0);
    turtle.color(drawille::PixelColor::White);
    turtle.down();

    // Print without disrupting existing output
    drawille_paste(&mut turtle.cvs.rows(), x+1, y+1);
    print!("{}", termion::color::Fg(termion::color::White));

}


fn draw_trigger (turtle: &mut drawille::Turtle, w: f32, h: f32, my_color: drawille::PixelColor, z: f32) {

    turtle.up();
    turtle.color(my_color);
    turtle.back(h/2.0);
    turtle.down();

    for i in 0..2 {
        turtle.forward(h);
        turtle.right(90.0);
        turtle.forward(w);
        turtle.right(90.0);
    }

    if z > 0.0 {
        turtle.up();
        turtle.right(90.0);
        turtle.forward(1.0);
        turtle.left(90.0);
        turtle.forward(h/2.0);
        turtle.down();

        turtle.color(drawille::PixelColor::White);

        let h = (h - 1.0) * z as f32;

        turtle.up();
        turtle.back(h/2.1);
        turtle.down();

        scribble(turtle, w - 1.0, h - 1.0, true);

        turtle.forward(h);
        turtle.color(my_color);
    }
}

