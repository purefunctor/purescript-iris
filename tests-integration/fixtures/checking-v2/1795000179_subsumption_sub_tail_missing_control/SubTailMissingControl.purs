module SubTailMissingControl where
foreign import x :: { a :: {} }
-- The same missing label with the row closed in the stored type.
z :: { a :: {}, b :: Int }
z = x
