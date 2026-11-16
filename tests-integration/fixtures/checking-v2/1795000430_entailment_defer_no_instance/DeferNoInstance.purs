module DeferNoInstance where

newtype Wrap f = Wrap f

class Convert f a b | f a -> b where
  convert :: f -> a -> b

instance Convert (Wrap f) a a where
  convert _ a = a

class Same a b | a -> b where
  same :: a -> b

deferred :: forall f x a b. Convert f a b => Same x a => f -> x -> b
deferred f x = convert f (same x)

rejected :: forall f. f -> Int
rejected f = convert f 1
