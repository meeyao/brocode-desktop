--!strict
-- Generated from crates/opencode-plugin-spec/contract.json by `cargo run -p opencode-plugin-spec -- --write`. Do not edit.
-- The host provides `@opencode/ui`; this file only gives luau-lsp its types. Contract types are global
-- (see opencode.d.luau).

export type Token = SemanticToken
export type Size = UiSize
export type Node = UiNode
export type Handler = UiHandler
export type Event = UiEvent
export type LayoutProps = UiLayoutProps
export type Pin = UiPin
export type BoxProps = UiBoxProps
export type FrameProps = UiFrameProps
export type TextProps = UiTextProps
export type SpacerProps = UiSpacerProps
export type DividerProps = UiDividerProps
export type IconProps = UiIconProps
export type ImageProps = UiImageProps
export type VideoProps = UiVideoProps
export type MetricProps = UiMetricProps
export type ButtonProps = UiButtonProps
export type IconButtonProps = UiIconButtonProps
export type SwitchProps = UiSwitchProps
export type CheckboxProps = UiCheckboxProps
export type RadioProps = UiRadioProps
export type FieldProps = UiFieldProps
export type SegmentedItem = UiSegmentedItem
export type ListboxProps = UiListboxProps
export type OptionProps = UiOptionProps
export type SelectOption = UiSelectOption
export type SelectProps = UiSelectProps
export type SegmentedProps = UiSegmentedProps
export type LoaderProps = UiLoaderProps
export type ServerIndicatorProps = UiServerIndicatorProps
export type PillProps = UiPillProps
export type KeybindProps = UiKeybindProps
export type SettingsRowProps = UiSettingsRowProps
export type SettingsListProps = UiSettingsListProps
export type FlyoutRowProps = UiFlyoutRowProps
export type BadgeProps = UiBadgeProps
export type DialogHeaderProps = UiDialogHeaderProps
export type MarkdownProps = UiMarkdownProps
export type VirtualListProps = UiVirtualListProps
export type TerminalSurfaceProps = UiTerminalSurfaceProps
export type TerminalTabLabelProps = UiTerminalTabLabelProps
export type BrowserSurfaceProps = UiBrowserSurfaceProps
export type CodeViewProps = UiCodeViewProps
export type DiffViewProps = UiDiffViewProps
export type FileTreeProps = UiFileTreeProps
export type PanelSidebarProps = UiPanelSidebarProps
export type OpenInButtonProps = UiOpenInButtonProps
export type OfficePagesProps = UiOfficePagesProps
export type OfficeSheetProps = UiOfficeSheetProps
export type GenUiProps = UiGenUiProps
export type ProgressCircleProps = UiProgressCircleProps
export type Align = UiAlign
export type Justify = UiJustify
export type Direction = UiDirection
export type EventKind = UiEventKind

local ui: UiLibrary = nil :: any
return ui
