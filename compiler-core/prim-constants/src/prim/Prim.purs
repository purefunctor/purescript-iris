module Prim where

data Type :: Type
data Type

data Function :: Type -> Type -> Type
data Function a b
type role Function representational representational

data Array :: Type -> Type
data Array a
type role Array representational

data Record :: Row Type -> Type
data Record a
type role Record representational

data Number :: Type
data Number

data Int :: Type
data Int

data String :: Type
data String

data Char :: Type
data Char

data Boolean :: Type
data Boolean

class Partial :: Constraint
class Partial

data Constraint :: Type
data Constraint

data Symbol :: Type
data Symbol

data Row :: Type -> Type
data Row a
type role Row representational
