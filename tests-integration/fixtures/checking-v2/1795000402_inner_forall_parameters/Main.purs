module Main where

data List a = Nil | Cons a (List a)

type NaturalTransformation f g = forall a. f a -> g a

infixr 4 type NaturalTransformation as ~>

drop :: forall a. Int -> List a -> List a
drop _ xs = xs

slice :: Int -> Int -> List ~> List
slice start _ xs = drop start xs

lambda :: Int -> List ~> List
lambda = \start xs -> drop start xs

bound :: Int -> List ~> List
bound _ = go
  where
  go xs = xs

leak :: Int -> List ~> List
leak _ _ = Cons 1 Nil
