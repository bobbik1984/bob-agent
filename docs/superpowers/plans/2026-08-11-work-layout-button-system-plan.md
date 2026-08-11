# Work Layout and Button System Implementation Plan

## Goal

Implement the approved single-flow Work page and a predictable button hierarchy without adding dependencies or changing Bob's product architecture.

## Batch 1: Shared responsive state

1. Add a pure layout resolver for terminal kind, viewport shape, and layout mode.
2. Provide the resolved state from `App.vue` while keeping the existing compact-navigation compatibility flag.
3. Cover native mobile landscape, desktop portrait, and desktop landscape with unit tests.

## Batch 2: Work page information architecture

1. Replace the nested project rail and detail columns with one centered content stream.
2. Remove the redundant “项目” heading and duplicate empty state.
3. Render desktop project choices as a compact horizontal switcher with solid accent and hollow inactive dots.
4. Render native mobile choices as a compact selector.
5. Make the outer page the only scrolling and spacing owner.

## Batch 3: Button contract

1. Extend the shared button primitives with primary, secondary, compact, selected, danger-outline, and icon states.
2. Migrate Work page actions to those primitives.
3. Remove page-local button definitions that conflict with the shared contract.

## Batch 4: High-conflict shared components

1. Align global dialogs with the shared hierarchy while preserving explicit destructive confirmations.
2. Remove duplicate button rules from confirmation and browser permission cards.
3. Audit remaining button implementations and record deferred migrations instead of making unsafe bulk changes.

## Batch 5: Documentation and verification

1. Record the layout and button rules in the project development documentation.
2. Run focused unit tests, the full test suite, and the production web build.
3. Inspect Work at desktop landscape, desktop portrait, and native-mobile dimensions.

