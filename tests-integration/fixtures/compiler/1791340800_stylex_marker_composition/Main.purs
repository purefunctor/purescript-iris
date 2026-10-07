module Main where

import Iris.StyleX as StyleX

rowMarker :: StyleX.Marker
rowMarker = StyleX.defineMarker

styles = StyleX.create { row: { color: "red" } }

rowProps :: StyleX.Props
rowProps = StyleX.props [ styles.row, rowMarker ]

rowAttrs :: StyleX.Attrs
rowAttrs = StyleX.attrs [ rowMarker, styles.row ]
