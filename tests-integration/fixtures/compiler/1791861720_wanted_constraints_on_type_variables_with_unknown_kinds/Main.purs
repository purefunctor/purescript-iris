module Main where

import Type.Proxy (Proxy)

type Invalid = Int String

class Check :: Row Type -> Constraint
class Check row

needsCheck :: forall row. Check row => Proxy row -> Int
needsCheck _ = 0

usesRow :: forall (row :: Row Invalid). Proxy row -> Int
usesRow value = needsCheck value
