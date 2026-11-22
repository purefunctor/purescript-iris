module UnexportedDuplicate (exported) where

exported :: Int
exported = 0

hidden :: Int
hidden = 1

between :: Int
between = 2

hidden :: Int
hidden = 3
