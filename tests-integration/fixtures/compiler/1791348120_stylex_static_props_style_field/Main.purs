module Main where

import Iris.StyleX as StyleX

styles = StyleX.create { base: { color: "red" } }

baseStyle = (StyleX.props styles.base).style
