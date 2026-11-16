module LabelOrder where

foreign import u :: forall t. { a :: Int, b :: t }
foreign import useInt :: Int -> Int

pick :: forall r. Array { a :: Int, b :: r } -> r
pick _ = pick []

same :: forall z. z -> z
same z = z

-- The array literal is inferred, so its elements are unified with unifyTypes.
-- Label a fails (Int vs String). Label b would solve t := String. A second error,
-- String vs Int at useInt, can appear only if label b was unified after label a failed.
test = useInt (pick (same [u, { a: "s", b: "str" }]))
