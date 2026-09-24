// Parallel checking allocates many small objects across Rayon workers.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    std::process::exit(iris_cli::run());
}
