module Main where

type role BeforeType representational
data BeforeType a = BeforeType a

data AfterValue a = AfterValue a

value :: Int
value = 0

type role AfterValue representational

data KindOnly :: Type -> Type
type role KindOnly representational
data KindOnly a = KindOnly a

data Adjacent a = Adjacent a
type role Adjacent representational

foreign import data Opaque :: Type -> Type
type role Opaque representational
