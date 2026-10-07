module Main where

import Iris.StyleX as StyleX

rowMarker :: StyleX.Marker
rowMarker = StyleX.defineMarker

styles = StyleX.create { row: { color: "red" } }

rowProps :: StyleX.Props
rowProps = StyleX.props [ styles.row, StyleX.markerStyle rowMarker ]

rowAttrs :: StyleX.Attrs
rowAttrs = StyleX.attrs [ StyleX.markerStyle rowMarker, styles.row ]
