module RecDupFirst where
foreign import r :: { a :: Int, a :: String }
foreign import len :: String -> Int
-- RecDupRun with the binders swapped: the first binder for label a is used as a String.
-- At runtime both binders read r.a, which holds the Int of the first label.
result :: Int
result = case r of
  { a: s, a: _ } -> len s
