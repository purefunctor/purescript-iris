module Iris.Effect where

data Sync :: Effects -> Type -> Type
data Sync effects value

type role Sync nominal representational

data Async :: Effects -> Type -> Type
data Async effects value

type role Async nominal representational

data Fiber :: Type -> Type
data Fiber value

type role Fiber representational
