module Data.Generic.Rep where

data NoConstructors

data NoArguments = NoArguments

data Sum a b = Inl a | Inr b

data Product a b = Product a b

newtype Constructor :: Symbol -> Type -> Type
newtype Constructor name a = Constructor a

newtype Argument a = Argument a

class Generic a rep | a -> rep where
  to :: rep -> a
  from :: a -> rep
