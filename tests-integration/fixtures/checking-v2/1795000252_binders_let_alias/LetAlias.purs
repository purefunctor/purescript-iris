module LetAlias where
foreign import idp :: forall a. a -> a
foreign import both :: Int -> String -> Int
-- A let-bound alias of a polymorphic value, used at two types.
t :: Int
t = let f = idp in both (f 1) (f "a")
