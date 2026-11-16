module SubTailPolyControl where
foreign import x :: forall r. { a :: Record r | r }
w :: { a :: { c :: forall b. b -> b }, c :: forall b. b -> b }
w = x
-- With the polytype field visible on both sides, label c is common and is subsumed.
z :: { a :: { c :: forall b. b -> b }, c :: Int -> Int }
z = w
