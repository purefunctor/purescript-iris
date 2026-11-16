module SubTailPoly where
foreign import x :: forall r. { a :: Record r | r }
-- Label a solves ?r := (c :: forall b. b -> b). Label c is then only in the solved tail,
-- so it is unified with Int -> Int instead of subsumed.
z :: { a :: { c :: forall b. b -> b }, c :: Int -> Int }
z = x
