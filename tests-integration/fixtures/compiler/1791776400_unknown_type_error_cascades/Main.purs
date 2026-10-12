module Main where

import Data.Show (show)

add :: Int -> Int -> Int
add value _ = value

unresolvedSignature :: Missing
unresolvedSignature = 0

usesUnresolvedSignature :: Int
usesUnresolvedSignature = add unresolvedSignature 1

invalidSignature :: Int String
invalidSignature = 0

usesInvalidSignature :: Int
usesInvalidSignature = add invalidSignature 1

unresolvedValue = missing

showsUnresolvedValue :: String
showsUnresolvedValue = show missing
