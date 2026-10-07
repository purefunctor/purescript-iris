module Main where

import Iris.StyleX as StyleX
import Iris.StyleX.When as When

styles = StyleX.create
  { base: { color: "red" }
  , sized: \(size :: { width :: Int, height :: String }) -> { width: size.width, height: size.height }
  , faded: \opacity -> { opacity: StyleX.conditionalValue opacity [ When.ancestor ":hover" 1.0 ] }
  }

sizedProps :: StyleX.DynamicProps
sizedProps = StyleX.props (styles.sized { width: 100, height: "50%" })

fadedProps = StyleX.props [ StyleX.dynamicStyle styles.base, styles.faded 0.5 ]

fadedAttrs = StyleX.attrs (styles.faded 0.25)

baseProps :: StyleX.Props
baseProps = StyleX.props styles.base
