module Main where

import Prim (Int)
import Alpha
import Beta
import Reexporter
import Alpha as O
import Alpha as LongAlias
import Alpha as Mb
import Alpha as Ma
import Alpha as M
import Beta as M

data Int = LocalInt
data Foo = LocalFoo

example :: { "hyphen-label" :: Prim.Int, "" :: Prim.String, "_" :: Prim.Boolean }
example =
  let
    primInt = 1
    localInt = LocalInt
    primString = "hello"
    otherFoo = O.Foo
    ambiguousBox = Ma.BoxA
    reexportedShared = Shared
    quotedRecord = { "hyphen-label": primInt, "": primString, "_": true }
  in
    keep localInt otherFoo ambiguousBox reexportedShared quotedRecord

keep :: forall a b c d e. a -> b -> c -> d -> e -> e
keep _ _ _ _ e = e
