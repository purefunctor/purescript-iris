module Main where

import Iris.StyleX as StyleX
import Record as Record
import Type.Proxy (Proxy(..))

base :: { color :: String, padding :: Int }
base = { color: "red", padding: 4 }

styles = StyleX.create
  { primary: Record.set (Proxy :: Proxy "color") "blue" base
  , spaced: Record.insert (Proxy :: Proxy "margin") 2 base
  }
