// Parallel checking allocates many small objects across Rayon workers.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    iris_cli::run()
}
