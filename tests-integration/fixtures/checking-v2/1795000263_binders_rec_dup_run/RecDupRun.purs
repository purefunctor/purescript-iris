module RecDupRun where
foreign import r :: { a :: Int, a :: String }
foreign import len :: String -> Int
-- A record binder that names label a twice, on a row with two a labels.
-- At runtime both binders read the same JS property r.a.
result :: Int
result = case r of
  { a: _, a: s } -> len s
