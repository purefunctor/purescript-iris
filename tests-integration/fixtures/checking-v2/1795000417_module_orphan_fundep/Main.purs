module Main where

import Convert (class Convert, class Pair)

data Local = Local

instance convertIntLocal :: Convert Int Local

instance convertLocalInt :: Convert Local Int

instance pairIntLocal :: Pair Int Local
