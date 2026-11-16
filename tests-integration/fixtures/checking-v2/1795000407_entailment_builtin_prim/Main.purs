module Main where

import Prim.Int as Int
import Prim.Row as Row
import Prim.RowList as RowList
import Prim.Symbol as Symbol
import Prim.TypeError (class Fail, class Warn, Text)

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

append :: forall left right appended. Symbol.Append left right appended => Proxy left -> Proxy right -> Proxy appended
append _ _ = Proxy

appended = append (Proxy :: Proxy "ab") (Proxy :: Proxy "cd")

suffix = (\right -> append (Proxy :: Proxy "ab") right) :: forall right. Symbol.Append "ab" right "abcd" => Proxy right -> Proxy "abcd"

prefixMismatch :: Proxy "xycd"
prefixMismatch = append (Proxy :: Proxy "ab") (Proxy :: Proxy "cd")

compareSymbols :: forall left right ordering. Symbol.Compare left right ordering => Proxy left -> Proxy right -> Proxy ordering
compareSymbols _ _ = Proxy

compared = compareSymbols (Proxy :: Proxy "a") (Proxy :: Proxy "b")

uncons :: forall first rest whole. Symbol.Cons first rest whole => Proxy whole -> Proxy first
uncons _ = Proxy

unconsed = uncons (Proxy :: Proxy "abc")

unconsEmpty = uncons (Proxy :: Proxy "")

add :: forall left right sum. Int.Add left right sum => Proxy left -> Proxy right -> Proxy sum
add _ _ = Proxy

added = add (Proxy :: Proxy 2) (Proxy :: Proxy 3)

subtracted :: Proxy 5
subtracted = add (Proxy :: Proxy 2) (Proxy :: Proxy 3)

wrongSum :: Proxy 6
wrongSum = add (Proxy :: Proxy 2) (Proxy :: Proxy 3)

multiply :: forall left right product. Int.Mul left right product => Proxy left -> Proxy right -> Proxy product
multiply _ _ = Proxy

multiplied = multiply (Proxy :: Proxy 4) (Proxy :: Proxy 5)

render :: forall value rendered. Int.ToString value rendered => Proxy value -> Proxy rendered
render _ = Proxy

rendered = render (Proxy :: Proxy 42)

field :: forall label value tail row. Row.Cons label value tail row => Proxy label -> Proxy row -> Proxy tail
field _ _ = Proxy

rest = field (Proxy :: Proxy "x") (Proxy :: Proxy (x :: Int, y :: String))

lacks :: forall label row. Row.Lacks label row => Proxy label -> Proxy row -> Int
lacks _ _ = 0

absent = lacks (Proxy :: Proxy "z") (Proxy :: Proxy (x :: Int))

present = lacks (Proxy :: Proxy "x") (Proxy :: Proxy (x :: Int))

lacksTail :: forall (row :: Row Type). Row.Lacks "z" row => Proxy row -> Int
lacksTail row = lacks (Proxy :: Proxy "z") (Proxy :: Proxy (x :: Int | row))

union :: forall left right union. Row.Union left right union => Proxy left -> Proxy right -> Proxy union
union _ _ = Proxy

unioned = union (Proxy :: Proxy (a :: Int, b :: Int)) (Proxy :: Proxy (b :: String, c :: Char))

nub :: forall input output. Row.Nub input output => Proxy input -> Proxy output
nub _ = Proxy

nubbed = nub (Proxy :: Proxy (a :: Int, a :: String, b :: Char))

toList :: forall row list. RowList.RowToList row list => Proxy row -> Proxy list
toList _ = Proxy

listed = toList (Proxy :: Proxy (b :: String, a :: Int))

failing :: Fail (Text "no instance for you") => Int
failing = 0

usesFailing :: Int
usesFailing = failing

warning :: Warn (Text "deprecated") => Int
warning = 0

usesWarning :: Int
usesWarning = warning
