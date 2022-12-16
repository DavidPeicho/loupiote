// Native entry point.
fn main() {
    let setup = pollster::block_on(standalone::setup());
    standalone::run(setup);
}
