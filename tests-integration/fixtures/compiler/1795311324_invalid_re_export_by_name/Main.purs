module Main where

import Files (ByteCount(..), fileSize)

size :: Number
size = case fileSize of
  ByteCount n -> n
