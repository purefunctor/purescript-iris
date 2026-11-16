module RecDup where
-- A record binder that names one label twice.
f :: { a :: Int } -> Int
f { a: x, a: y } = x
