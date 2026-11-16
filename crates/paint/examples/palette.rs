//! Prints the studio palettes from their recipe (`Palette::generate`) as Rust, to paste over
//! `Palette::STUDIO_DARK` and `Palette::STUDIO_LIGHT` if the recipe changes, with each colour's
//! contrast beside it:
//!
//! ```sh
//! cargo run -p lp-paint --example palette
//! ```

use lp_paint::{Palette, contrast};

fn main() {
    for dark in [true, false] {
        let p = Palette::generate(dark);
        println!("// {}", p.name);
        for (name, c) in p.slots() {
            let on = if name.starts_with("on.") || ["raised", "edge", "faint", "dim", "text"].contains(&name)
            {
                format!("{:5.1}:1 on the ground", contrast(c, p.ground))
            } else if name == "ground" || name == "well" {
                String::new()
            } else {
                format!("{:5.1}:1 on the well", contrast(c, p.well))
            };
            println!("    {name:10} 0x{c:06x},   // {on}");
        }
        println!();
    }
}
