use std::io::Write;

fn main() {
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "aipm-pack {}", libaipm::version());
}
