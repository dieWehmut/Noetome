"""Independent allowlist of visually measured reader capabilities.

The entries here come from the recorded native acceptance of the frozen author
cases, not from a product plan, report, capability flag or Rust implementation.
Each verifier still validates its own profile schema and source semantics before
asking this module about the target. A supported renderer never establishes
formula meaning, arrow direction, source legibility or label completeness.

The previously measured Obsidian 1.6.7 combinations remain registered. B06V-T03
also measured the precise Obsidian 1.15.0 / MathJax 4.1.3 pair with native,
relational and clear-scan author cases in both themes, and the exact Schema 2
Pele flow profile with branch, punctuation and signed-relation cases. Its SVG
target passed separate source-label, native screenshot/RGBA and attachment
acceptance in both themes. Other entries require their own acceptance;
no version range, inferred engine compatibility or flag grants access.
"""
from __future__ import annotations


_NATIVE_MATH_TARGETS = frozenset({("Obsidian", "1.6.7", "3.2.2"),
                                  ("Obsidian", "1.15.0", "4.1.3")})
_SVG_TARGETS = frozenset({("Obsidian", "1.6.7"), ("Obsidian", "1.15.0")})
_FLOWCHART_TARGETS = frozenset({(
    1, "flow_render_profile", "obsidian_1_6_7_flowchart", "Obsidian",
    "1.6.7", "10.6.1", "supported", "decimal_ascii_punctuation_v1",
)})
_FLOWCHART_RENDERER_TARGETS = frozenset({(
    "flow_render_profile", "obsidian_1_15_0_pele_flowchart", "Obsidian", "1.15.0",
    "supported", "decimal_ascii_punctuation_v1", "pele", "1.8.0",
    "9bf34b3a9a19351dc69d0d238e365560331a8ae392c41656df7f5090653253d0",
)})


def supports_native_math(profile):
    """Exact app/MathJax pair plus explicitly enabled, verified native math."""
    return (type(profile["schema_version"]) is int and profile["schema_version"] == 1
            and profile["verified"] is True and profile["native_math"] is True
            and (profile["app"], profile["app_version"], profile["mathjax_version"])
            in _NATIVE_MATH_TARGETS)


def supports_svg_target(profile):
    """Measured SVG target identity; the caller checks the separate SVG flag.

    Keeping the target and flag decisions separate preserves both diagnostics
    when an unknown target also declares SVG unavailable.
    """
    return (type(profile["schema_version"]) is int and profile["schema_version"] == 1
            and profile["verified"] is True
            and (profile["app"], profile["app_version"]) in _SVG_TARGETS)


def supports_flowchart(profile):
    """Exact versioned engine identity, wire codec and measured label setting."""
    if type(profile["schema_version"]) is not int:
        return False
    if profile["schema_version"] == 1:
        return (profile["html_labels"] is True and tuple(profile[field] for field in (
                "schema_version", "producer", "profile_id", "app", "app_version",
                "mermaid_version", "flowchart_support", "label_encoding",
            )) in _FLOWCHART_TARGETS)
    if profile["schema_version"] == 2:
        renderer = profile["renderer"]
        return (profile["mermaid_version"] is None and profile["html_labels"] is False
                and tuple(profile[field] for field in (
                    "producer", "profile_id", "app", "app_version", "flowchart_support", "label_encoding",
                )) + tuple(renderer[field] for field in ("name", "version", "source_sha256"))
                in _FLOWCHART_RENDERER_TARGETS)
    return False
