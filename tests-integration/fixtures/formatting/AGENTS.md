## Line widths

Formatting fixtures should show the layouts that people see in real code. Give their `-- @format`
headers common line widths: 80, 100, or 120 columns. A header without `width` uses the default of
100.

When the behavior involves wrapping, write source that is long enough to wrap at these widths. Do
not narrow the width to make short source wrap. Combine `indent` and `unicode` variants with these
widths too.

To test a width boundary, keep the width fixed and size the source around it. For example, one
declaration can fill exactly 80 columns while another exceeds 80 by one column.

Widths below 80 are stress tests. They belong in dedicated fixtures whose names state the
narrow-width behavior under test, such as comments or indivisible atoms that are wider than the
line. Do not add narrow configurations to other fixtures. Fixtures that test option validation use
the invalid values they reject.

The harness also checks fixtures at narrow widths without recording snapshots. Keep these hidden
stress tests when changing fixture widths.
