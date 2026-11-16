module SubOverwrite where
foreign import x :: forall r. { a :: Record r | r }
-- PB-1.1's constraint through subsumes: label a solves ?r := (b :: Int), the tail then needs (b :: String).
z :: { a :: { b :: Int }, b :: String }
z = x
