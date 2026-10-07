module Main where

import Iris.StyleX as StyleX

styles = StyleX.create { sized: \wide -> { width: if wide then 100 else 50 } }
