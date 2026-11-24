module Main where

import Prim.Row (class Cons, class Union)

class C (r :: Row Type)

class T (r :: Row Type)

instance C r => T r

class Pick a (r :: Row Type) | a -> r

instance Pick String ()

instance (Cons "x" Int r' r, Pick String r') => Pick Int r

data P (r :: Row Type) = P

elim :: forall a picked rest all. Pick a picked => Union picked rest all => T rest => a -> P all -> Int
elim _ _ = 0

test :: forall r. T r => P (x :: Int | r) -> Int
test = elim 1
