pub const FUEL: u32 = 1_000_000;

#[macro_export]
macro_rules! safe_loop {
    ($($body:tt)*) => {{
        let mut fuel = 0u32;
        loop {
            if fuel >= $crate::safety::FUEL {
                unreachable!("invariant violated: fuel exhausted");
            }
            fuel += 1;
            $($body)*
        }
    }};
}
