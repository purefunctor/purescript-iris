module ArgsDup where
data B = T | F
-- The same name in two columns of a refutable equation.
f :: B -> B -> B
f x x = x
