module Main where

import Iris.StyleX as StyleX

base = StyleX.create { root: { color: "red" } }

properties = StyleX.props base.root

styles = StyleX.create { root: { color: properties.className } }
