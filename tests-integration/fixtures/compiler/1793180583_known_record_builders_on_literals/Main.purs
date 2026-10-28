module Main where

import Prelude

import Record.Builder (build, buildFromScratch, delete, insert, merge, modify, rename)
import Type.Proxy (Proxy(..))

inserted :: Int -> { x :: Int, y :: Int, z :: Int }
inserted value = build (insert (Proxy :: _ "z") value) { x: 1, y: 2 }

reshaped :: Int -> { x :: Int, w :: Int, z :: String }
reshaped value = build
  (modify (Proxy :: _ "x") (_ + value) >>> delete (Proxy :: _ "y") >>> rename (Proxy :: _ "v") (Proxy :: _ "w") >>> insert (Proxy :: _ "z") "new")
  { x: 1, y: 2, v: 3 }

composedBackwards :: { x :: Int, y :: Int }
composedBackwards = build (modify (Proxy :: _ "y") (_ * 10) <<< insert (Proxy :: _ "y") 4) { x: 1 }

fromScratch :: Int -> { x :: Int, y :: Int }
fromScratch value = buildFromScratch (insert (Proxy :: _ "x") value >>> insert (Proxy :: _ "y") 2)

merged :: { x :: Int, y :: Int, z :: Int }
merged = build (merge { y: 20, z: 30 }) { x: 1, y: 2 }
