#!/usr/bin/env python3
"""
Iteratively trim unused features from ALL external dependencies in Cargo.toml.

Phase 1 - Strip & Rebuild (per dependency):
  1. Set default-features = false, remove all features
  2. If cargo check passes -> done, no features needed
  3. If fails -> get default features from cargo metadata, add them all + original features
  4. If still fails -> restore original completely

Phase 2 - Minimize (per dependency that has features):
  For each feature in (default features + original features), try removing it:
  - If check still passes -> permanently remove it
  - If check fails -> keep it, restore it

Result: every dependency has default-features = false with only the minimal
set of explicitly listed features that are actually needed.
"""

import json
import re
import subprocess
import sys
import os
import shutil

ROOT_DIR = os.path.dirname(os.path.abspath(__file__))
CARGO_TOML = os.path.join(ROOT_DIR, "Cargo.toml")
BACKUP_TOML = os.path.join(ROOT_DIR, "Cargo.toml.bak")
REQUIRED_FEATURES = {
    # reqwest HTTPS support is a runtime requirement. cargo check cannot detect
    # the missing TLS backend because the failure only appears when sending
    # requests to https:// endpoints.
    "reqwest": ["rustls"],
}


def read_file(path):
    with open(path, "r") as f:
        return f.read()


def write_file(path, content):
    with open(path, "w") as f:
        f.write(content)


def run_check():
    """Run cargo check --workspace, return (success, output)."""
    result = subprocess.run(
        ["cargo", "check", "--workspace"],
        capture_output=True,
        text=True,
        timeout=300,
        cwd=ROOT_DIR,
    )
    output = result.stdout + result.stderr
    return result.returncode == 0, output


def get_default_features():
    """Get default features for all packages via cargo metadata."""
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        capture_output=True, text=True, cwd=ROOT_DIR,
    )
    data = json.loads(result.stdout)
    default_feats = {}
    for pkg in data["packages"]:
        feats = pkg.get("features", {}).get("default", [])
        if feats:
            default_feats[pkg["name"]] = feats
    return default_feats


def find_dep_line_index(content, section_header, dep_name):
    """Find the line index of a dependency in a specific TOML section."""
    lines = content.split('\n')
    in_section = False
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped == section_header:
            in_section = True
            continue
        if in_section:
            if stripped.startswith('[') and not stripped.startswith('#'):
                break
            if re.match(rf'^{re.escape(dep_name)}\s*=', stripped):
                return i
    return None


def parse_dep_line(line):
    """Parse a dependency line. Returns (name, 'table', inner) or (name, 'string', version) or Nones."""
    m = re.match(r'^([\w-]+)\s*=\s*\{(.*)\}\s*$', line.strip())
    if m:
        return m.group(1), 'table', m.group(2).strip()
    m = re.match(r'^([\w-]+)\s*=\s*"([^"]*)"\s*$', line.strip())
    if m:
        return m.group(1), 'string', m.group(2)
    return None, None, None


def extract_features(inner):
    """Extract features list from inner string."""
    m = re.search(r'features\s*=\s*\[([^\]]*)\]', inner)
    if not m:
        return []
    return re.findall(r'"([^"]*)"', m.group(1))


def has_key(inner, key):
    """Check if a key exists in inner string."""
    return bool(re.search(rf'{re.escape(key)}\s*=', inner))


def has_path(inner):
    """Check if dep has path = ... (internal dep)."""
    return has_key(inner, "path")


def has_default_features_false(inner):
    """Check if default-features = false is set."""
    return bool(re.search(r'default-features\s*=\s*false', inner))


def make_inner_no_features_no_defaults(inner):
    """Strip features and default-features from inner, set default-features = false."""
    inner = re.sub(r',?\s*features\s*=\s*\[[^\]]*\]', '', inner)
    inner = re.sub(r',?\s*default-features\s*=\s*(?:true|false)', '', inner)
    # Clean leading comma
    inner = re.sub(r'^\s*,\s*', '', inner)
    inner = inner.rstrip() + ', default-features = false'
    return inner.strip()


def build_table_line(name, inner):
    """Build dependency line as inline table."""
    return f'{name} = {{ {inner} }}'


def build_line_with_features(name, base_inner, features):
    """Build a line with default-features = false and the given features list."""
    inner = make_inner_no_features_no_defaults(base_inner)
    if features:
        feat_str = ", ".join(f'"{f}"' for f in features)
        inner = inner + f', features = [{feat_str}]'
    return build_table_line(name, inner)


def string_to_inner(version):
    """Convert version string to inner table content."""
    return f'version = "{version}"'


def apply_line(content, section, dep_name, new_line):
    """Apply a line change to content, return new content."""
    idx = find_dep_line_index(content, section, dep_name)
    if idx is None:
        print(f"    ERROR: Could not find {dep_name} in {section}")
        return content
    lines = content.split('\n')
    lines[idx] = new_line
    return '\n'.join(lines)


def collect_all_deps(content):
    """Collect ALL external deps (not path deps) from workspace.dependencies and build-dependencies."""
    lines = content.split('\n')
    deps = []
    current_section = None

    for i, line in enumerate(lines):
        stripped = line.strip()

        if stripped == '[workspace.dependencies]':
            current_section = 'workspace'
            continue
        elif stripped == '[build-dependencies]':
            current_section = 'build'
            continue
        elif stripped.startswith('[') and not stripped.startswith('#'):
            current_section = None
            continue

        if current_section and stripped and not stripped.startswith('#'):
            name, dtype, inner = parse_dep_line(stripped)
            if name is None:
                continue

            if dtype == 'table' and has_path(inner):
                continue

            section = '[build-dependencies]' if current_section == 'build' else '[workspace.dependencies]'

            if dtype == 'table':
                deps.append({
                    'name': name,
                    'section': section,
                    'type': 'table',
                    'features': extract_features(inner),
                    'inner': inner,
                    'line': stripped,
                })
            elif dtype == 'string':
                deps.append({
                    'name': name,
                    'section': section,
                    'type': 'string',
                    'version': inner,
                    'features': [],
                    'inner': string_to_inner(inner),
                    'line': stripped,
                })

    return deps


def process_dep(dep, default_features_map):
    """Process a single dependency.
    Returns (original_features, needed_features) where needed_features is the minimal set."""
    name = dep['name']
    section = dep['section']
    original_features = dep['features']
    base_inner = dep['inner']
    required_features = REQUIRED_FEATURES.get(name, [])

    print(f"\n{'='*60}")
    print(f"Processing: {name} (in {section})")
    print(f"  Original: {dep['line']}")
    print(f"{'='*60}")

    # --- Phase 1: Strip and rebuild ---

    # Step 1: Strip to default-features = false, no features
    stripped_line = build_table_line(name, make_inner_no_features_no_defaults(base_inner))
    content = read_file(CARGO_TOML)
    content = apply_line(content, section, name, stripped_line)
    write_file(CARGO_TOML, content)
    print(f"  Stripped to: {stripped_line}")

    print("  Running cargo check...")
    sys.stdout.flush()
    success, _ = run_check()
    if success:
        print("  PASS - No features needed!")
        return original_features, []

    print("  FAIL - Rebuilding with default + original features...")

    # Step 2: Add default features + original features, try check
    default_feats = default_features_map.get(name, [])
    # Merge: default features first, then original features (deduplicated, preserving order)
    all_features = list(default_feats)
    for f in required_features:
        if f not in all_features:
            all_features.append(f)
    for f in original_features:
        if f not in all_features:
            all_features.append(f)

    rebuilt_line = build_line_with_features(name, base_inner, all_features)
    content = read_file(CARGO_TOML)
    content = apply_line(content, section, name, rebuilt_line)
    write_file(CARGO_TOML, content)
    print(f"  Rebuilt with features: {all_features}")

    print("  Running cargo check...")
    sys.stdout.flush()
    success, _ = run_check()
    if not success:
        # Complete failure - restore original
        print("  FAIL even with all features - restoring original")
        content = read_file(CARGO_TOML)
        content = apply_line(content, section, name, dep['line'])
        write_file(CARGO_TOML, content)
        return original_features, list(all_features)

    print(f"  PASS with {len(all_features)} features")

    # --- Phase 2: Minimize - try removing each feature one by one ---
    print("  Minimizing features...")
    needed = list(all_features)

    for feature in list(all_features):
        if feature in required_features:
            print(f"    Kept:    {feature} (required)")
            continue

        trial = [f for f in needed if f != feature]
        trial_line = build_line_with_features(name, base_inner, trial)
        content = read_file(CARGO_TOML)
        content = apply_line(content, section, name, trial_line)
        write_file(CARGO_TOML, content)

        sys.stdout.flush()
        success, _ = run_check()
        if success:
            needed = trial
            print(f"    Removed: {feature}")
        else:
            print(f"    Kept:    {feature}")

    # Write final state
    final_line = build_line_with_features(name, base_inner, needed)
    content = read_file(CARGO_TOML)
    content = apply_line(content, section, name, final_line)
    write_file(CARGO_TOML, content)

    print(f"  Final: {needed}")
    return original_features, needed


def main():
    shutil.copy2(CARGO_TOML, BACKUP_TOML)
    print(f"Backup saved to {BACKUP_TOML}")

    # Pre-fetch default features
    print("Fetching default features from cargo metadata...")
    sys.stdout.flush()
    default_features_map = get_default_features()
    print(f"Got default features for {len(default_features_map)} packages")

    content = read_file(CARGO_TOML)
    deps = collect_all_deps(content)

    print(f"\nFound {len(deps)} external dependencies to process:")
    for d in deps:
        feat_str = f", features={d['features']}" if d['features'] else ""
        print(f"  [{d['section']}] {d['name']} ({d['type']}{feat_str})")

    results = {}
    for dep in deps:
        original, needed = process_dep(dep, default_features_map)
        removed_from_orig = [f for f in original if f not in needed]
        new_from_default = [f for f in needed if f not in original]
        results[dep['name']] = {
            'original': original,
            'needed': needed,
            'removed': removed_from_orig,
            'new_from_default': new_from_default,
        }

    print(f"\n\n{'='*60}")
    print("FINAL SUMMARY")
    print(f"{'='*60}")
    for name, r in results.items():
        parts = []
        if r['removed']:
            parts.append(f"REMOVED: {r['removed']}")
        if r['new_from_default']:
            parts.append(f"FROM DEFAULT: {r['new_from_default']}")
        if r['needed']:
            parts.append(f"FINAL: {r['needed']}")
        else:
            parts.append("No features needed")
        print(f"  {name}: {' | '.join(parts)}")

    total_original = sum(len(r['original']) for r in results.values())
    total_final = sum(len(r['needed']) for r in results.values())
    total_from_default = sum(len(r['new_from_default']) for r in results.values())
    print(f"\nOriginal features: {total_original}, Final: {total_final}, From default-features: {total_from_default}")

    print("\nRunning final cargo check...")
    success, _ = run_check()
    if success:
        print("Final cargo check: PASS")
    else:
        print("Final cargo check: FAIL - restoring backup!")
        shutil.copy2(BACKUP_TOML, CARGO_TOML)


if __name__ == "__main__":
    main()
