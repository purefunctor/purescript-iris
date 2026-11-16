module TOpenOpen where

-- Type-level twin of KOpenOpen: the shared fresh tail is kept, so this must be TypesDoNotUnify.
foreign import p :: forall r s. { a1 :: { a :: Int | r }, a2 :: { b :: Int | s }, z1 :: { | r }, z2 :: { | s } }

same :: forall z. { a1 :: z, a2 :: z, z1 :: { b :: Int }, z2 :: { a :: Int, c :: Int } } -> Int
same _ = 0

test = same p
