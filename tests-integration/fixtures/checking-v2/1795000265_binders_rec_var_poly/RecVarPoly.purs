module RecVarPoly where
foreign import both :: Int -> String -> Int
-- Control: a variable binder on a polymorphic record field stays polymorphic.
h :: { x :: forall a. a -> a } -> Int
h { x: g } = both (g 1) (g "a")
