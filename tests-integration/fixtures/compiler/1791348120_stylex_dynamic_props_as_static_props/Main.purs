module Main where

import Iris.StyleX as StyleX

styles = StyleX.create { sized: \(width :: Int) -> { width } }

sizedProps :: StyleX.Props
sizedProps = StyleX.props (styles.sized 4)
