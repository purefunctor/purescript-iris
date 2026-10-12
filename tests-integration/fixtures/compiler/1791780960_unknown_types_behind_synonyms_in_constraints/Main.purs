module Main where

class Constrained :: Type -> Constraint
class Constrained a

type Invalid = Int String

type Wrapped = Array Invalid

type Loop = Array Loop

needsInvalid :: Constrained Invalid => Int
needsInvalid = 0

usesInvalid :: Int
usesInvalid = needsInvalid

needsWrapped :: Constrained Wrapped => Int
needsWrapped = 0

usesWrapped :: Int
usesWrapped = needsWrapped

needsLoop :: Constrained Loop => Int
needsLoop = 0

usesLoop :: Int
usesLoop = needsLoop
