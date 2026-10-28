module Main where

import Iris.StyleX as StyleX
import Record as Record
import Type.Proxy (Proxy(..))

card :: { color :: String, padding :: String }
card = StyleX.defineConsts { color: "navy", padding: "8px" }

styles = StyleX.create
  { root: Record.set (Proxy :: Proxy "color") "teal" card
  }
