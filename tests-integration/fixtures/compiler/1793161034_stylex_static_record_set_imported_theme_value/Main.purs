module Main where

import Iris.StyleX as StyleX
import Record as Record
import Tokens as Tokens
import Type.Proxy (Proxy(..))

styles = StyleX.create
  { root: Record.set (Proxy :: Proxy "color") "teal" Tokens.card
  }
