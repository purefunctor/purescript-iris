module Main where

import Prim.Row (class Union)
import Type.Proxy (Proxy)

type Invalid = Int String

class Check :: Row Type -> Constraint
class Check row

needsCheck :: forall row. Check row => Proxy row -> Int
needsCheck _ = 0

usesAnnotatedGiven :: forall (row :: Row Type). Union (x :: Invalid) () row => Proxy row -> Int
usesAnnotatedGiven value = needsCheck value

usesInferredGiven :: forall row. Union (x :: Invalid) () row => Proxy row -> Int
usesInferredGiven value = needsCheck value

usesNoGiven :: forall (row :: Row Type). Proxy row -> Int
usesNoGiven value = needsCheck value
