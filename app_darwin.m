#import <Cocoa/Cocoa.h>
#import <QuartzCore/QuartzCore.h>

// C callbacks (implemented in Rust via #[no_mangle]).
extern void goOnReady(void);
extern void goProviderSelected(const char *providerID);
extern void goRefreshRequested(void);
extern void goSaveCredentials(const char *provider, const char *field, const char *value);
extern void goSaveCodexAccounts(const char *accountsJSON);
extern void goSaveProviderEnabled(const char *providersJSON);
extern void goSignIn(const char *provider);
extern void goCancelSignIn(const char *provider);
extern void goSetAccount(const char *provider, const char *key, const char *label, bool shown);
extern void goCodexUseAccount(const char *key);
extern void goRemoveAccount(const char *provider, const char *key);
extern void goSetSelectedAccount(const char *provider, const char *key);
extern void goStartWindow(const char *provider, const char *key);
extern void goSaveRefreshMinutes(unsigned int minutes);
extern void goQuitRequested(void);
extern void goSetSystemLanguage(const char *tag);
extern void goSaveLanguage(const char *setting);

// ---------------------------------------------------------------------------
// Language. Rust sends every string it builds already translated, and says
// which language is in effect (`language` in the state); the strings this file
// draws itself go through OCGT, keyed by their English text.
// ---------------------------------------------------------------------------

static BOOL gOCGChinese = NO;

static NSString *OCGT(NSString *english) {
  if (!gOCGChinese) return english;
  static NSDictionary<NSString *, NSString *> *zh;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    zh = @{
    @"Usage" : @"用量",
    @"waiting for data" : @"等待数据",
    @"%@ isn't set up yet." : @"%@ 尚未设置。",
    @"OPEN SETTINGS" : @"打开设置",
    @"Starting" : @"正在开启",
    @"Codex now uses" : @"Codex 现在使用",
    @"not configured" : @"未配置",
    @"Sure?" : @"确定？",
    @"Use" : @"切换",
    @"Click again to sign Codex in to this account" : @"再点一次，让 Codex 登录此账户",
    @"Use this account in Codex — the one it uses now is kept here instead" : @"让 Codex 使用此账户——它当前的账户会保存在这里",
    @"The account the Codex CLI is signed in to" : @"Codex CLI 当前登录的账户",
    @"Start 5h window" : @"开启 5 小时窗口",
    @"Starting the 5h window…" : @"正在开启 5 小时窗口…",
    @"Start the 5h window now (sends one tiny request)" : @"立即开启 5 小时窗口（发送一个很小的请求）",
    @"Driving the menu bar badge — click to un-pin" : @"正在决定菜单栏数字——点击取消固定",
    @"Click to pin to the menu bar badge" : @"点击固定为菜单栏数字",
    @"Pinned" : @"已固定",
    @"Last refreshed" : @"上次刷新",
    @"Refresh" : @"刷新",
    @"Preferences" : @"偏好设置",
    @"Quit" : @"退出",
    @"OpenCode Go windows. Sign in with your OpenCode console account, or use an API key." : @"OpenCode Go 用量窗口。用 OpenCode 控制台账户登录，或使用 API key。",
    @"Account balance, read with an API key from platform.deepseek.com." : @"账户余额，用 platform.deepseek.com 的 API key 读取。",
    @"Token Plan windows, read with the plan's Subscription Key." : @"Token Plan 用量窗口，用套餐的 Subscription Key 读取。",
    @"ChatGPT subscription windows for Codex, for every account you sign in." : @"Codex 的 ChatGPT 订阅用量窗口，覆盖你登录的每个账户。",
    @"commandcode.ai credits and 5h / weekly windows." : @"commandcode.ai 积分，以及 5 小时 / 每周窗口。",
    @"claude.ai subscription windows — Claude Code's login, and any you add." : @"claude.ai 订阅用量窗口——Claude Code 的登录，以及你添加的账户。",
    @"the key set below" : @"下方设置的 key",
    @"in Codex" : @"Codex 使用中",
    @"Claude Code's login" : @"Claude Code 的登录",
    @"the CLI's login" : @"CLI 的登录",
    @"signed in here" : @"在 tokue 中登录",
    @"Shown in the menu bar popover" : @"显示在菜单栏弹窗中",
    @"Switched off" : @"已关闭",
    @"General" : @"通用",
    @"PROVIDERS" : @"提供商",
    @"How every provider's quotas are read, and how often they are fetched." : @"额度怎么显示、多久获取一次，对所有提供商生效。",
    @"METERS" : @"计量",
    @"USED" : @"已用",
    @"REMAINING" : @"剩余",
    @"Read quotas as" : @"额度显示为",
    @"The menu bar number and every bar in the popover follow this." : @"菜单栏数字和弹窗里的每个进度条都按此显示。",
    @"REFRESH" : @"刷新",
    @"Minutes between background refreshes (1–240)" : @"后台刷新间隔，单位分钟（1–240）",
    @"min" : @"分钟",
    @"Refresh every" : @"刷新间隔",
    @"The refresh button in the popover fetches right away." : @"弹窗里的刷新按钮会立即获取。",
    @"LANGUAGE" : @"语言",
    @"Language" : @"界面语言",
    @"Menus, cards and messages. System follows macOS." : @"菜单、卡片和提示文字。跟随系统即使用 macOS 的语言。",
    @"SYSTEM" : @"跟随系统",
    @"Show in the popover" : @"在弹窗中显示",
    @"Off hides the tab and stops fetching it." : @"关闭后隐藏它的标签页，并停止获取。",
    @"ACCOUNTS" : @"账户",
    @"No account yet." : @"还没有账户。",
    @"API KEY" : @"API KEY",
    @"API KEY (OPTIONAL)" : @"API KEY（可选）",
    @"Saved when you leave the field" : @"离开输入框时保存",
    @"CARD ROWS" : @"卡片行",
    @"Spend limit" : @"支出上限",
    @"The workspace spend-control meter, when it is in use." : @"工作区的支出控制额度（启用时显示）。",
    @"Today's usage" : @"今日用量",
    @"How much of the 5h window went since midnight." : @"从零点起用掉了多少 5 小时窗口。",
    @"Reset credits" : @"重置次数",
    @"How many free window resets the account holds." : @"账户还有几次免费的窗口重置。",
    @"Click to rename — shown on the card instead of the email" : @"点击重命名——卡片上会代替邮箱显示",
    @"The CLI's own login — tokue reads it but never changes it; sign out with the CLI." : @"CLI 自己的登录——tokue 只读取、从不修改；要退出请用 CLI。",
    @"Show this account in the popover" : @"在弹窗中显示此账户",
    @"REMOVE" : @"移除",
    @"Forget this account's login" : @"删除此账户的登录",
    @"USE IN CODEX" : @"用于 CODEX",
    @"Sign Codex in to this account; its current one is kept here instead" : @"让 Codex 登录此账户；它当前的账户会保存在这里",
    @"Confirm this code in the browser" : @"请在浏览器中确认此代码",
    @"COPY" : @"复制",
    @"OPEN PAGE" : @"打开页面",
    @"CANCEL" : @"取消",
    @"ADD ACCOUNT" : @"添加账户",
    @"Sign in to another account in the browser" : @"在浏览器中登录另一个账户",
    @"this account" : @"此账户",
    @"Use %@ in Codex?" : @"让 Codex 使用 %@？",
    @"Codex will be signed in to %@.%@ Codex sessions already running keep their account until they are restarted." : @"Codex 将登录 %@。%@正在运行的 Codex 会话会继续使用原账户，直到重启。",
    @" %@ is kept here as a saved account." : @"%@ 会作为已保存的账户留在这里。",
    @"Use in Codex" : @"用于 Codex",
    @"Cancel" : @"取消",
    @"Remove %@?" : @"移除 %@？",
    @"Its login is deleted from tokue. To track it again, add it with Add Account." : @"它的登录会从 tokue 中删除。要重新跟踪，请用“添加账户”再加回来。",
    @"Remove" : @"移除",
    };
  });
  return zh[english] ?: english;
}

// ---------------------------------------------------------------------------
// Fixed dark "terminal" palette — the popover and the Preferences window are
// a deliberate always-dark, phosphor-tinted surface, not a system-adaptive
// one: this look is the point of the redesign, so it does not follow light
// mode. (The real menu-bar title text stays on the adaptive palette above —
// it renders in the system menu bar itself, not inside our surface.)
// ---------------------------------------------------------------------------

static NSColor *OCGPanelColor(void) { // popover / window surface
  return [NSColor colorWithSRGBRed:0.114 green:0.133 blue:0.122 alpha:1.0];
}
static NSColor *OCGSurfaceInsetColor(void) { // title bar / sidebar strip
  return [NSColor colorWithSRGBRed:0.086 green:0.098 blue:0.090 alpha:1.0];
}
static NSColor *OCGCardColor(void) { // account cards, avatar chips
  return [NSColor colorWithSRGBRed:0.145 green:0.165 blue:0.151 alpha:1.0];
}
/// The frame around the card currently driving the menu bar badge.
static NSColor *OCGSelectedBorderColor(void) {
  return [NSColor colorWithSRGBRed:0.34 green:0.66 blue:0.45 alpha:1.0];
}

static NSColor *OCGBorderColor(void) {
  return [NSColor colorWithSRGBRed:0.216 green:0.243 blue:0.224 alpha:1.0];
}
static NSColor *OCGTrackColor(void) { // unfilled bar / toggle-off track
  return [NSColor colorWithSRGBRed:0.235 green:0.263 blue:0.243 alpha:1.0];
}
static NSColor *OCGTextPrimary(void) {
  return [NSColor colorWithSRGBRed:0.918 green:0.941 blue:0.925 alpha:1.0];
}
static NSColor *OCGTextSecondary(void) {
  return [NSColor colorWithSRGBRed:0.53 green:0.58 blue:0.545 alpha:1.0];
}
static NSColor *OCGTextTertiary(void) {
  return [NSColor colorWithSRGBRed:0.38 green:0.42 blue:0.395 alpha:1.0];
}
static NSColor *OCGAccentGreen(void) { // plenty of quota left
  return [NSColor colorWithSRGBRed:0.40 green:0.88 blue:0.56 alpha:1.0];
}
static NSColor *OCGAccentAmber(void) { // less than 30% left
  return [NSColor colorWithSRGBRed:0.86 green:0.66 blue:0.24 alpha:1.0];
}
static NSColor *OCGAccentRed(void) { // less than 10% left
  return [NSColor colorWithSRGBRed:0.88 green:0.36 blue:0.32 alpha:1.0];
}

/// Fixed-palette equivalent of `OCGStatusColor`, for views drawn on the dark
/// popover/Preferences surface.
static NSColor *OCGStatusColorFixed(int used) {
  if (used > 90) {
    return OCGAccentRed();
  }
  if (used > 70) {
    return OCGAccentAmber();
  }
  return OCGAccentGreen();
}

/// The redesign's typeface throughout the popover and Preferences window.
static NSFont *OCGMonoFont(CGFloat size, NSFontWeight weight) {
  return [NSFont monospacedSystemFontOfSize:size weight:weight];
}

@interface OCGFlipView : NSView
@end

@implementation OCGFlipView
- (BOOL)isFlipped {
  return YES;
}
@end

// ---------------------------------------------------------------------------
// OCGCardView: an account card whose whole surface selects the card.
//
// The card is built in two stages — the title row inside cardViewWithTitle:,
// then the meter rows appended by the caller afterwards — so a full-size
// invisible "hit area" button can never simply be added last and stay in
// front. AppKit's hit-testing returns whichever subview's frame contains the
// point regardless of whether that subview handles clicks, so any label or
// meter row added later silently swallows clicks over the area it covers.
// Routing hit-testing here instead makes the whole card clickable no matter
// what gets added to it later; everything inside a card is display-only.
// ---------------------------------------------------------------------------

@interface OCGCardView : NSView
@property(weak) NSButton *hitArea;
@end

@implementation OCGCardView
- (NSView *)hitTest:(NSPoint)point {
  NSView *hit = [super hitTest:point];
  if (hit == nil || self.hitArea == nil) {
    return hit; // outside the card, or a card with nothing to select
  }
  // A real button inside the card (e.g. "start window") keeps its own click;
  // everything else — labels, bars — selects the card. The hit area covers
  // the whole card in front of those buttons, so super would report it for
  // their spot too: look for them directly.
  NSPoint local = [self convertPoint:point fromView:self.superview];
  for (NSView *sub in self.subviews) {
    if (sub != self.hitArea && !sub.hidden && [sub isKindOfClass:[NSButton class]] &&
        NSPointInRect(local, sub.frame)) {
      return sub;
    }
  }
  return self.hitArea;
}
@end

// ---------------------------------------------------------------------------
// OCGToggle: a small pill switch matching the popover's phosphor palette.
// NSSwitch always tints its "on" state with the user's system accent colour
// (blue by default), not our custom green — that's what broke visual
// consistency between the Preferences window and the popover's hand-drawn
// bars/buttons. This draws its own track + knob instead.
// ---------------------------------------------------------------------------

@interface OCGToggle : NSButton
/// Our own on/off flag — deliberately NOT `NSControl.state`. A push-on/
/// push-off or momentary NSButtonCell resets `state` right after the action
/// fires as part of its own click-tracking (that's what "momentary" means),
/// silently discarding anything the handler sets — confirmed by hand with a
/// standalone `-performClick:` harness before landing this. `isOn` is a
/// plain ivar the cell never touches, so nothing can reset it out from
/// under us.
@property(nonatomic) BOOL isOn;
@property(strong) CALayer *knobLayer;
/// The caller's real target/action — `-setTarget:`/`-setAction:` below store
/// here instead of on the button itself, so every click is intercepted by
/// `-handleClick:` first (flip `isOn` ourselves) before forwarding.
@property(weak) id forwardTarget;
@property SEL forwardAction;
@end

@implementation OCGToggle

- (instancetype)init {
  self = [super initWithFrame:NSZeroRect];
  if (self) {
    self.buttonType = NSButtonTypeMomentaryChange;
    self.bordered = NO;
    self.title = @"";
    self.translatesAutoresizingMaskIntoConstraints = NO;
    self.wantsLayer = YES;
    self.layer.cornerRadius = 8;
    _knobLayer = [CALayer layer];
    _knobLayer.backgroundColor = [NSColor colorWithWhite:0.92 alpha:1.0].CGColor;
    [self.layer addSublayer:_knobLayer];
    [super setTarget:self];
    [super setAction:@selector(handleClick:)];
    [NSLayoutConstraint activateConstraints:@[
      [self.widthAnchor constraintEqualToConstant:28],
      [self.heightAnchor constraintEqualToConstant:16],
    ]];
  }
  return self;
}

- (void)setTarget:(id)target {
  self.forwardTarget = target;
}

- (id)target {
  return self.forwardTarget;
}

- (void)setAction:(SEL)action {
  self.forwardAction = action;
}

- (SEL)action {
  return self.forwardAction;
}

- (void)setIsOn:(BOOL)isOn {
  _isOn = isOn;
  [self setNeedsLayout:YES];
}

- (void)handleClick:(id)sender {
  self.isOn = !self.isOn;
  id t = self.forwardTarget;
  SEL a = self.forwardAction;
  if (t != nil && a != NULL) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
    [t performSelector:a withObject:self];
#pragma clang diagnostic pop
  }
}

- (void)layout {
  [super layout];
  BOOL on = self.isOn;
  CGFloat pad = 2;
  CGFloat knob = self.bounds.size.height - pad * 2;
  CGFloat x = on ? self.bounds.size.width - knob - pad : pad;
  [CATransaction begin];
  [CATransaction setDisableActions:YES];
  self.knobLayer.frame = NSMakeRect(x, pad, knob, knob);
  self.knobLayer.cornerRadius = knob / 2;
  self.layer.backgroundColor = (on ? OCGAccentGreen() : OCGTrackColor()).CGColor;
  self.layer.borderWidth = on ? 0 : 1;
  self.layer.borderColor = OCGBorderColor().CGColor;
  [CATransaction commit];
}

- (void)setState:(NSControlStateValue)state {
  [super setState:state];
  [self setNeedsLayout:YES];
}

@end

// ---------------------------------------------------------------------------
// OCGModePicker: a two-way pill switch (Used / Remaining) — the same idea as
// OCGToggle, for NSSegmentedControl, whose selected-segment tint is also the
// system accent colour rather than our palette.
// ---------------------------------------------------------------------------

@interface OCGModePicker : NSView
@property(nonatomic) NSInteger selectedSegment;
@property(weak) id target;
@property SEL action;
- (instancetype)initWithLabels:(NSArray<NSString *> *)labels;
@end

@implementation OCGModePicker {
  NSArray<NSButton *> *_segmentButtons;
}

- (instancetype)initWithLabels:(NSArray<NSString *> *)labels {
  self = [super initWithFrame:NSZeroRect];
  if (self) {
    self.translatesAutoresizingMaskIntoConstraints = NO;
    self.wantsLayer = YES;
    self.layer.cornerRadius = 6;
    self.layer.backgroundColor = OCGSurfaceInsetColor().CGColor;

    NSMutableArray<NSButton *> *buttons = [NSMutableArray array];
    NSView *previous = nil;
    for (NSUInteger i = 0; i < labels.count; i++) {
      NSButton *b = [NSButton buttonWithTitle:labels[i] target:self action:@selector(segmentClicked:)];
      b.tag = (NSInteger)i;
      b.bordered = NO;
      b.wantsLayer = YES;
      b.layer.cornerRadius = 5;
      b.translatesAutoresizingMaskIntoConstraints = NO;
      [self addSubview:b];
      [buttons addObject:b];
      [NSLayoutConstraint activateConstraints:@[
        [b.topAnchor constraintEqualToAnchor:self.topAnchor constant:2],
        [b.bottomAnchor constraintEqualToAnchor:self.bottomAnchor constant:-2],
      ]];
      if (previous == nil) {
        [b.leadingAnchor constraintEqualToAnchor:self.leadingAnchor constant:2].active = YES;
      } else {
        [b.leadingAnchor constraintEqualToAnchor:previous.trailingAnchor constant:2].active = YES;
        [b.widthAnchor constraintEqualToAnchor:previous.widthAnchor].active = YES;
      }
      previous = b;
    }
    [previous.trailingAnchor constraintEqualToAnchor:self.trailingAnchor constant:-2].active = YES;
    _segmentButtons = buttons;
    [self updateAppearance];
  }
  return self;
}

- (void)setSelectedSegment:(NSInteger)selectedSegment {
  _selectedSegment = selectedSegment;
  [self updateAppearance];
}

- (void)segmentClicked:(NSButton *)sender {
  self.selectedSegment = sender.tag;
  if (self.target != nil && self.action != NULL) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
    [self.target performSelector:self.action withObject:self];
#pragma clang diagnostic pop
  }
}

- (void)updateAppearance {
  for (NSUInteger i = 0; i < _segmentButtons.count; i++) {
    NSButton *b = _segmentButtons[i];
    BOOL selected = ((NSInteger)i == self.selectedSegment);
    b.layer.backgroundColor = (selected ? OCGAccentGreen() : [NSColor clearColor]).CGColor;
    b.attributedTitle = [[NSAttributedString alloc]
        initWithString:b.title
            attributes:@{
              NSFontAttributeName : [NSFont monospacedSystemFontOfSize:9.5
                                                                  weight:selected ? NSFontWeightBold
                                                                                  : NSFontWeightMedium],
              NSForegroundColorAttributeName : selected ? OCGPanelColor() : OCGTextSecondary(),
            }];
  }
}

@end

// ---------------------------------------------------------------------------
// UsagePanelController: two-column popover content (sidebar + content area).
// Declared before OCGAppDelegate so the latter can alloc/init it.
// ---------------------------------------------------------------------------

// Panel typography: every label in the right-hand pane is drawn this many
// points smaller than its nominal size, so the whole pane scales from here.
static const CGFloat kPanelFontDelta = -2.0;
static const CGFloat kMinFontSize = 9.0;
/// 弹窗默认最小总高（含头部与底栏）：内容再少也不缩成一小条。
// ---------------------------------------------------------------------------
// Type ramp. Six sizes that used to sit within 7pt of each other — 9, 10 and
// 11 were indistinguishable — re-spaced so each step is a visible one (about
// 1.15x apart) and every size is a whole point, since fractional sizes render
// soft in the monospaced faces used throughout.
// ---------------------------------------------------------------------------
static const CGFloat kOCGFontTitle = 17;      // provider name, Preferences pane heading
static const CGFloat kOCGFontCardTitle = 13;  // account name on a card
static const CGFloat kOCGFontBody = 12;       // meter labels, settings rows
static const CGFloat kOCGFontDetail = 11;     // countdowns, paths, timestamps
static const CGFloat kOCGFontBadge = 9;       // plan pill

static const CGFloat kMinPanelHeight = 320.0;

/// Shared by every view built on the dark surface — the popover's
/// UsagePanelController and the Preferences window's
/// SettingsWindowController both call these instead of duplicating layout
/// boilerplate.
static NSTextField *OCGLabel(NSString *text, CGFloat size, NSFontWeight weight, NSColor *color) {
  NSTextField *label = [NSTextField labelWithString:text ?: @""];
  label.font = [NSFont monospacedSystemFontOfSize:MAX(kMinFontSize, size + kPanelFontDelta) weight:weight];
  label.textColor = color;
  label.lineBreakMode = NSLineBreakByTruncatingTail;
  label.translatesAutoresizingMaskIntoConstraints = NO;
  return label;
}

/// Centered attributed title for a flat/filled button — `NSButton` won't
/// take a plain text colour once its bezel supplies one.
static NSAttributedString *OCGButtonTitle(NSString *text, NSColor *color, NSFontWeight weight) {
  NSMutableParagraphStyle *style = [[NSMutableParagraphStyle alloc] init];
  style.alignment = NSTextAlignmentCenter;
  NSDictionary *attrs = @{
    NSFontAttributeName : [NSFont monospacedSystemFontOfSize:10.5 weight:weight],
    NSForegroundColorAttributeName : color,
    NSParagraphStyleAttributeName : style,
  };
  return [[NSAttributedString alloc] initWithString:text attributes:attrs];
}

/// Primary CTA: a filled, phosphor-green rounded button. A custom layer fill
/// (not `bezelColor`) so it renders identically live and in the off-screen
/// `TOKUE_SNAPSHOT` window, which never becomes key.
static void OCGStylePrimaryButton(NSButton *button, NSString *title) {
  button.bordered = NO;
  button.wantsLayer = YES;
  button.layer.cornerRadius = 6;
  button.layer.backgroundColor = OCGAccentGreen().CGColor;
  button.attributedTitle = OCGButtonTitle(title, OCGPanelColor(), NSFontWeightBold);
}

/// Secondary action: an outlined button on the dark surface.
static void OCGStyleSecondaryButton(NSButton *button, NSString *title) {
  button.bordered = NO;
  button.wantsLayer = YES;
  button.layer.cornerRadius = 6;
  button.layer.borderWidth = 1;
  button.layer.borderColor = OCGBorderColor().CGColor;
  button.attributedTitle = OCGButtonTitle(title, OCGTextSecondary(), NSFontWeightSemibold);
}

/// Credential fields the Preferences window's Providers pane should show
/// beneath a given provider's row (empty when that tool manages its own
/// credentials, e.g. Codex/Command Code reading their CLI's own auth file).
static NSArray *OCGFieldsForProvider(NSString *provider) {
  if ([provider isEqualToString:@"codex"] || [provider isEqualToString:@"commandcode"] ||
      [provider isEqualToString:@"claude"]) {
    return @[]; // these read the CLI's own login; nothing to type in
  }
  return @[ @{@"field" : @"api_key", @"label" : @"API Key", @"secure" : @YES} ];
}

/// Pin `row` to `content`'s full width, stacking it under `*previous` (or at
/// the top when `*previous` is nil), then advance `*previous` to `row`.
static void OCGAddRowTo(NSView *content, NSView *row, CGFloat height, NSView **previous, CGFloat gap) {
  row.translatesAutoresizingMaskIntoConstraints = NO;
  [content addSubview:row];
  NSMutableArray *constraints = [NSMutableArray array];
  [constraints addObject:[row.leadingAnchor constraintEqualToAnchor:content.leadingAnchor constant:16]];
  [constraints addObject:[row.trailingAnchor constraintEqualToAnchor:content.trailingAnchor constant:-16]];
  [constraints addObject:[row.heightAnchor constraintEqualToConstant:height]];
  if (*previous == nil) {
    // Was hardcoded to 10 regardless of what callers asked for — the first
    // row's `topGap` was silently ignored, which is exactly why the usage
    // view's "Updated ..." row (topGap: 4) sat 10pt down instead of 4.
    [constraints addObject:[row.topAnchor constraintEqualToAnchor:content.topAnchor constant:gap]];
  } else {
    [constraints addObject:[row.topAnchor constraintEqualToAnchor:(*previous).bottomAnchor constant:gap]];
  }
  [NSLayoutConstraint activateConstraints:constraints];
  *previous = row;
}

/// Just enough of OCGAppDelegate for the popover to ask for the Preferences
/// window without needing its full @interface declared this early in the file.
@protocol OCGPreferencesOpening <NSObject>
- (void)showPreferencesWindow;
@end

@interface UsagePanelController : NSViewController
@property(strong) NSView *sidebar;
@property(strong) NSView *content;
@property(strong) NSView *contentContainer;
@property(strong) NSView *headerHolder;
@property(strong) NSScrollView *scrollView;
@property(strong) NSLayoutConstraint *scrollHeightConstraint;
/// The Codex account whose "Use" was just pressed once; a second press within
/// a few seconds switches Codex to it. Nothing else is confirmed this way.
@property(copy) NSString *pendingSwitchKey;
/// Content height at the last layout, to detect growth.
@property CGFloat lastContentHeight;
@property(strong) NSMutableDictionary<NSString *, NSButton *> *providerButtons;
@property(strong) NSDictionary *state;
/// So the gear button can ask for the standalone Preferences window —
/// settings no longer lives inside this popover.
@property(weak) id<OCGPreferencesOpening> appDelegate;
- (void)renderAll;
@end

@implementation UsagePanelController

- (instancetype)init {
  self = [super initWithNibName:nil bundle:nil];
  if (self) {
    _providerButtons = [NSMutableDictionary dictionary];
  }
  return self;
}

- (void)loadView {
  NSRect frame = NSMakeRect(0, 0, 272, kMinPanelHeight);
  self.view = [[NSView alloc] initWithFrame:frame];
  // Single unified background across the whole popover — a fixed dark
  // "terminal" surface, not the system-adaptive window background: this is
  // the redesign's whole point, so it does not follow light mode.
  self.view.wantsLayer = YES;
  self.view.layer.backgroundColor = OCGPanelColor().CGColor;

  // A horizontal tab strip across the top switches providers — replaces the
  // old left-edge icon rail, which ate into content width on every screen
  // regardless of which provider you were looking at and read as a bolted-on
  // afterthought next to the hand-drawn cards below it.
  self.sidebar = [[NSView alloc] initWithFrame:NSZeroRect];
  self.sidebar.translatesAutoresizingMaskIntoConstraints = NO;
  self.sidebar.wantsLayer = YES;
  self.sidebar.layer.backgroundColor = OCGSurfaceInsetColor().CGColor;
  [self.view addSubview:self.sidebar];

  NSView *tabBarDivider = [[NSView alloc] initWithFrame:NSZeroRect];
  tabBarDivider.translatesAutoresizingMaskIntoConstraints = NO;
  tabBarDivider.wantsLayer = YES;
  tabBarDivider.layer.backgroundColor = OCGBorderColor().CGColor;
  [self.view addSubview:tabBarDivider];

  // Header and footer stay put; only the usage rows scroll — four ChatGPT
  // accounts plus their meters are taller than the popover should ever be.
  self.headerHolder = [[NSView alloc] initWithFrame:NSZeroRect];
  self.headerHolder.translatesAutoresizingMaskIntoConstraints = NO;
  [self.view addSubview:self.headerHolder];

  self.scrollView = [[NSScrollView alloc] initWithFrame:NSZeroRect];
  self.scrollView.translatesAutoresizingMaskIntoConstraints = NO;
  self.scrollView.drawsBackground = NO;
  self.scrollView.hasVerticalScroller = YES;
  self.scrollView.autohidesScrollers = YES;
  self.scrollView.borderType = NSNoBorder;
  [self.view addSubview:self.scrollView];

  self.content = [[OCGFlipView alloc] initWithFrame:NSZeroRect];
  self.content.translatesAutoresizingMaskIntoConstraints = NO;
  self.contentContainer = self.content; // the scroll view's document view
  self.scrollView.documentView = self.content;
  [self.content.widthAnchor constraintEqualToAnchor:self.scrollView.contentView.widthAnchor]
      .active = YES;

  self.scrollHeightConstraint = [self.scrollView.heightAnchor constraintEqualToConstant:220];
  // Fixed width: long account titles truncate (with a tooltip) instead of
  // stretching the popover.
  [self.view.widthAnchor constraintEqualToConstant:312].active = YES;
  [NSLayoutConstraint activateConstraints:@[
    [self.sidebar.leadingAnchor constraintEqualToAnchor:self.view.leadingAnchor],
    [self.sidebar.trailingAnchor constraintEqualToAnchor:self.view.trailingAnchor],
    [self.sidebar.topAnchor constraintEqualToAnchor:self.view.topAnchor],
    [self.sidebar.heightAnchor constraintEqualToConstant:38],

    [tabBarDivider.leadingAnchor constraintEqualToAnchor:self.view.leadingAnchor],
    [tabBarDivider.trailingAnchor constraintEqualToAnchor:self.view.trailingAnchor],
    [tabBarDivider.topAnchor constraintEqualToAnchor:self.sidebar.bottomAnchor],
    [tabBarDivider.heightAnchor constraintEqualToConstant:1],

    [self.headerHolder.leadingAnchor constraintEqualToAnchor:self.view.leadingAnchor],
    [self.headerHolder.trailingAnchor constraintEqualToAnchor:self.view.trailingAnchor],
    [self.headerHolder.topAnchor constraintEqualToAnchor:tabBarDivider.bottomAnchor],
    [self.headerHolder.heightAnchor constraintEqualToConstant:38],

    [self.scrollView.leadingAnchor constraintEqualToAnchor:self.view.leadingAnchor],
    [self.scrollView.trailingAnchor constraintEqualToAnchor:self.view.trailingAnchor],
    [self.scrollView.topAnchor constraintEqualToAnchor:self.headerHolder.bottomAnchor],
    [self.scrollView.bottomAnchor constraintEqualToAnchor:self.view.bottomAnchor],
    self.scrollHeightConstraint,

    [self.content.topAnchor constraintEqualToAnchor:self.scrollView.contentView.topAnchor],
    [self.content.leadingAnchor constraintEqualToAnchor:self.scrollView.contentView.leadingAnchor],
  ]];
}

- (void)viewWillAppear {
  [super viewWillAppear];
  [self renderAll];
}

- (void)viewDidLayout {
  [super viewDidLayout];
  [self updatePreferredSize];
}

/// How tall the popover may grow: everything the screen has below the menu bar,
/// minus the popover arrow. Scrolling is only a fallback for absurdly long lists.
- (CGFloat)maxPanelHeight {
  NSScreen *screen = self.view.window.screen ?: [NSScreen mainScreen];
  CGFloat available = screen.visibleFrame.size.height - 24.0;
  return MAX(280.0, available);
}

/// Size the popover to its content — every account stays visible — and only
/// scroll when the list would not fit on the screen at all.
- (void)updatePreferredSize {
  CGFloat contentHeight = self.content.fittingSize.height;
  if (contentHeight <= 0) {
    // Width not resolved yet: lay out once so the row constraints can compute.
    [self.view layoutSubtreeIfNeeded];
    contentHeight = self.content.fittingSize.height;
  }
  if (contentHeight <= 0) {
    return;
  }
  CGFloat chrome = 77.0; // tab bar (38) + divider (1) + header (38)
  CGFloat scrollHeight = MIN(MAX(contentHeight, kMinPanelHeight - chrome),
                             [self maxPanelHeight] - chrome);
  if (fabs(self.scrollHeightConstraint.constant - scrollHeight) > 0.5) {
    self.scrollHeightConstraint.constant = scrollHeight;
    [self.view layoutSubtreeIfNeeded];
  }
  NSSize size = NSMakeSize(312, scrollHeight + chrome);
  if (!NSEqualSizes(self.preferredContentSize, size)) {
    self.preferredContentSize = size;
    if (getenv("TOKUE_DEBUG_SIZE") != NULL) {
      fprintf(stderr, "[ocg] 面板高度 -> %.0fpt (内容 %.0fpt, 屏幕可用 %.0fpt)\n",
              size.height, contentHeight, [self maxPanelHeight] - chrome);
    }
  }
  // The account list can be taller than the popover. Whenever the content
  // height changes (first paint, accounts appearing) snap back to the top so
  // the first account is never scrolled out of sight.
  if (fabs(self.lastContentHeight - contentHeight) > 0.5) {
    self.lastContentHeight = contentHeight;
    [self.scrollView.contentView scrollToPoint:NSMakePoint(0, 0)];
    [self.scrollView reflectScrolledClipView:self.scrollView.contentView];
  }
}

- (void)updateWithStateJSON:(NSString *)json {
  NSData *data = [json dataUsingEncoding:NSUTF8StringEncoding];
  NSDictionary *parsed = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
  if (![parsed isKindOfClass:[NSDictionary class]]) {
    return;
  }
  gOCGChinese = [parsed[@"language"] isEqual:@"zh-Hans"];
  self.state = parsed;
  if (self.viewIfLoaded == nil) {
    return;
  }
  [self renderAllWhenNotClicking];
}

/// A refresh landing mid-click tears the button out of the hierarchy while
/// its cell is still tracking the mouse, and the click is silently dropped —
/// indistinguishable, from the outside, from a card that just won't select.
/// Every click already triggers a state push of its own, so this collides
/// exactly when someone clicks through several cards in a row.
- (void)renderAllWhenNotClicking {
  if (([NSEvent pressedMouseButtons] & 1) != 0) {
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.1 * NSEC_PER_SEC)),
                   dispatch_get_main_queue(), ^{
                     [self renderAllWhenNotClicking];
                   });
    return;
  }
  [self renderAll];
}

- (void)renderAll {
  [self renderSidebar];
  [self renderUsage];
  // viewDidLayout only fires once (before the rows exist), so the popover has to
  // be told its new height here — otherwise it keeps the initial frame and clips
  // the account list.
  [self.view layoutSubtreeIfNeeded];
  [self updatePreferredSize];
}

// ---------- tab bar ----------

- (void)renderSidebar {
  for (NSView *view in [self.sidebar.subviews copy]) {
    [view removeFromSuperview];
  }
  [self.providerButtons removeAllObjects];

  NSArray *providers = self.state[@"providers"] ?: @[];
  NSString *active = self.state[@"active"] ?: @"";

  NSMutableArray<NSButton *> *buttons = [NSMutableArray array];
  for (NSDictionary *provider in providers) {
    NSString *providerID = provider[@"id"] ?: @"";
    if ([provider[@"enabled"] boolValue] == NO) {
      continue; // switched off in Preferences
    }
    NSButton *button = [NSButton buttonWithTitle:@""
                                          target:self
                                          action:@selector(providerClicked:)];
    button.image = OCGLogoImageForProvider(providerID);
    button.imagePosition = NSImageOnly;
    button.bordered = NO;
    BOOL isActive = [providerID isEqualToString:active];
    button.contentTintColor = isActive ? OCGAccentGreen() : OCGTextSecondary();
    button.identifier = providerID;
    button.translatesAutoresizingMaskIntoConstraints = NO;
    // The selected tab is marked by a thin lit outline, nothing else; the
    // rest carry no frame at all, so the bar reads as a row of marks with
    // one of them switched on rather than a row of buttons.
    button.wantsLayer = YES;
    button.layer.cornerRadius = 6;
    button.layer.borderWidth = isActive ? 0.5 : 0;
    button.layer.borderColor = isActive ? OCGAccentGreen().CGColor : NSColor.clearColor.CGColor;
    [button.widthAnchor constraintEqualToConstant:34].active = YES;
    [button.heightAnchor constraintEqualToConstant:29].active = YES;
    [buttons addObject:button];
    self.providerButtons[providerID] = button;
  }

  // A compact, centered row — not stretched to fill the bar — so it reads as
  // a tab group rather than icons pinned to arbitrary edges.
  NSStackView *stack = [NSStackView stackViewWithViews:buttons];
  stack.orientation = NSUserInterfaceLayoutOrientationHorizontal;
  stack.distribution = NSStackViewDistributionEqualSpacing;
  stack.alignment = NSLayoutAttributeCenterY;
  stack.spacing = 4;
  stack.translatesAutoresizingMaskIntoConstraints = NO;
  [self.sidebar addSubview:stack];
  [NSLayoutConstraint activateConstraints:@[
    [stack.centerXAnchor constraintEqualToAnchor:self.sidebar.centerXAnchor],
    [stack.centerYAnchor constraintEqualToAnchor:self.sidebar.centerYAnchor],
  ]];
}

static NSDictionary *OCGLogoSpecForProvider(NSString *providerID);

// OCGLogoImageForProvider returns a template NSImage built from the provider's
// brand logo SVG path (viewBox 0 0 24 24). Template mode lets contentTintColor
// recolour it for active/inactive states. Shared by the popover's tab bar and
// the Preferences window's Providers pane.
//
static NSImage *OCGLogoImageForProvider(NSString *providerID) {
  NSDictionary *spec = OCGLogoSpecForProvider(providerID);
  if (spec == nil) {
    return [NSImage imageWithSystemSymbolName:@"circle" accessibilityDescription:providerID];
  }
  NSString *svg = [NSString stringWithFormat:
      @"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"%@\" "
      @"fill=\"black\" fill-rule=\"evenodd\"><path d=\"%@\"/></svg>",
      spec[@"viewBox"] ?: @"0 0 24 24", spec[@"d"]];
  NSData *svgData = [svg dataUsingEncoding:NSUTF8StringEncoding];
  NSImage *image = [[NSImage alloc] initWithData:svgData];
  image.template = YES;
  image.size = NSMakeSize(17, 17);
  return image;
}

// OCGLogoSpecForProvider returns each provider's brand mark: the `d` of a
// single filled path, plus an optional `viewBox` for marks that are not
// authored on the usual 24x24 grid. Every mark here is the official one —
// the @lobehub/icons Mono variant (Codex uses OpenAI's), except Command
// Code's, which is taken from its own app icon (the inner glyph only; its
// rounded-square container fills in to a blob at menu bar sizes).
static NSDictionary *OCGLogoSpecForProvider(NSString *providerID) {
  static NSDictionary *paths = nil;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    paths = @{
      @"opencode" : @{
        @"d" : @"M16 6H8v12h8V6zm4 16H4V2h16v20z",
      },
      @"deepseek" : @{
        @"d" : @"M23.748 4.482c-.254-.124-.364.113-.512.234-.051.039-.094.09-.137.136-.372.397-.806.657-1"
              @".373.626-.829-.046-1.537.214-2.163.848-.133-.782-.575-1.248-1.247-1.548-.352-.156-.708-."
              @"311-.955-.65-.172-.241-.219-.51-.305-.774-.055-.16-.11-.323-.293-.35-.2-.031-.278.136-.3"
              @"56.276-.313.572-.434 1.202-.422 1.84.027 1.436.633 2.58 1.838 "
              @"3.393.137.093.172.187.129.323-.082.28-.18.552-.266.833-.055.179-.137.217-.329.14a5.526 "
              @"5.526 0 01-1.736-1.18c-.857-.828-1.631-1.742-2.597-2.458a11.365 11.365 0 00-.689-.471c-."
              @"985-.957.13-1.743.388-1.836.27-.098.093-.432-.779-.428-.872.004-1.67.295-2.687.684a3.055"
              @" 3.055 0 01-.465.137 9.597 9.597 0 00-2.883-.102c-1.885.21-3.39 1.102-4.497 2.623C.082 "
              @"8.606-.231 10.684.152 12.85c.403 2.284 1.569 4.175 3.36 5.653 1.858 1.533 3.997 2.284 "
              @"6.438 2.14 1.482-.085 3.133-.284 4.994-1.86.47.234.962.327 1.78.397.63.059 1.236-.03 "
              @"1.705-.128.735-.156.684-.837.419-.961-2.155-1.004-1.682-.595-2.113-.926 1.096-1.296 "
              @"2.746-2.642 3.392-7.003.05-.347.007-.565 0-.845-.004-.17.035-.237.23-.256a4.173 4.173 0 "
              @"001.545-.475c1.396-.763 1.96-2.015 2.093-3.517.02-.23-.004-.467-.247-.588zM11.581 18c-2."
              @"089-1.642-3.102-2.183-3.52-2.16-.392.024-.321.471-.235.763.09.288.207.486.371.739.114.16"
              @"7.192.416-.113.603-.673.416-1.842-.14-1.897-.167-1.361-.802-2.5-1.86-3.301-3.307-.774-1."
              @"393-1.224-2.887-1.298-4.482-.02-.386.093-.522.477-.592a4.696 4.696 0 "
              @"011.529-.039c2.132.312 3.946 1.265 5.468 2.774.868.86 1.525 1.887 2.202 2.891.72 1.066 "
              @"1.494 2.082 2.48 "
              @"2.914.348.292.625.514.891.677-.802.09-2.14.11-3.054-.614zm1-6.44a.306.306 0 "
              @"01.415-.287.302.302 0 01.2.288.306.306 0 01-.31.307.303.303 0 01-.304-.308zm3.11 "
              @"1.596c-.2.081-.399.151-.59.16a1.245 1.245 0 "
              @"01-.798-.254c-.274-.23-.47-.358-.552-.758a1.73 1.73 0 "
              @"01.016-.588c.07-.327-.008-.537-.239-.727-.187-.156-.426-.199-.688-.199a.559.559 0 "
              @"01-.254-.078c-.11-.054-.2-.19-.114-.358.028-.054.16-.186.192-.21.356-.202.767-.136 "
              @"1.146.016.352.144.618.408 "
              @"1.001.782.391.451.462.576.685.914.176.265.336.537.445.848.067.195-.019.354-.25.452z",
      },
      @"minimax" : @{
        @"d" : @"M16.278 2c1.156 0 2.093.927 2.093 2.07v12.501a.74.74 0 00.744.709.74.74 0 "
              @"00.743-.709V9.099a2.06 2.06 0 012.071-2.049A2.06 2.06 0 0124 9.1v6.561a.649.649 0 "
              @"01-.652.645.649.649 0 01-.653-.645V9.1a.762.762 0 00-.766-.758.762.762 0 "
              @"00-.766.758v7.472a2.037 2.037 0 01-2.048 2.026 2.037 2.037 0 "
              @"01-2.048-2.026v-12.5a.785.785 0 00-.788-.753.785.785 0 00-.789.752l-.001 15.904A2.037 "
              @"2.037 0 0113.441 22a2.037 2.037 0 01-2.048-2.026V18.04c0-.356.292-.645.652-.645.36 0 "
              @".652.289.652.645v1.934c0 .263.142.506.372.638.23.131.514.131.744 0a.734.734 0 "
              @"00.372-.638V4.07c0-1.143.937-2.07 2.093-2.07zm-5.674 0c1.156 0 2.093.927 2.093 "
              @"2.07v11.523a.648.648 0 01-.652.645.648.648 0 01-.652-.645V4.07a.785.785 0 "
              @"00-.789-.78.785.785 0 00-.789.78v14.013a2.06 2.06 0 01-2.07 2.048 2.06 2.06 0 "
              @"01-2.071-2.048V9.1a.762.762 0 00-.766-.758.762.762 0 00-.766.758v3.8a2.06 2.06 0 "
              @"01-2.071 2.049A2.06 2.06 0 010 12.9v-1.378c0-.357.292-.646.652-.646.36 0 "
              @".653.29.653.646V12.9c0 .418.343.757.766.757s.766-.339.766-.757V9.099a2.06 2.06 0 "
              @"012.07-2.048 2.06 2.06 0 012.071 2.048v8.984c0 .419.343.758.767.758.423 0 "
              @".766-.339.766-.758V4.07c0-1.143.937-2.07 2.093-2.07z",
      },
      @"codex" : @{
        @"d" : @"M9.205 8.658v-2.26c0-.19.072-.333.238-.428l4.543-2.616c.619-.357 1.356-.523 2.117-.523 "
              @"2.854 0 4.662 2.212 4.662 4.566 0 .167 0 .357-.024.547l-4.71-2.759a.797.797 0 00-.856 "
              @"0l-5.97 3.473zm10.609 8.8V12.06c0-.333-.143-.57-.429-.737l-5.97-3.473 "
              @"1.95-1.118a.433.433 0 01.476 0l4.543 2.617c1.309.76 2.189 2.378 2.189 3.948 0 1.808-1.07"
              @" 3.473-2.76 4.163zM7.802 12.703l-1.95-1.142c-.167-.095-.239-.238-.239-.428V5.899c0-2.545"
              @" 1.95-4.472 4.591-4.472 1 0 1.927.333 2.712.928L8.23 "
              @"5.067c-.285.166-.428.404-.428.737v6.898zM12 15.128l-2.795-1.57v-3.33L12 8.658l2.795 "
              @"1.57v3.33L12 15.128zm1.796 7.23c-1 "
              @"0-1.927-.332-2.712-.927l4.686-2.712c.285-.166.428-.404.428-.737v-6.898l1.974 "
              @"1.142c.167.095.238.238.238.428v5.233c0 2.545-1.974 4.472-4.614 "
              @"4.472zm-5.637-5.303l-4.544-2.617c-1.308-.761-2.188-2.378-2.188-3.948A4.482 4.482 0 "
              @"014.21 6.327v5.423c0 .333.143.571.428.738l5.947 3.449-1.95 1.118a.432.432 0 01-.476 "
              @"0zm-.262 3.9c-2.688 0-4.662-2.021-4.662-4.519 0-.19.024-.38.047-.57l4.686 "
              @"2.71c.286.167.571.167.856 0l5.97-3.448v2.26c0 .19-.07.333-.237.428l-4.543 "
              @"2.616c-.619.357-1.356.523-2.117.523zm5.899 2.83a5.947 5.947 0 005.827-4.756C22.287 "
              @"18.339 24 15.84 24 13.296c0-1.665-.713-3.282-1.998-4.448.119-.5.19-.999.19-1.498 "
              @"0-3.401-2.759-5.947-5.946-5.947-.642 0-1.26.095-1.88.31A5.962 5.962 0 0010.205 0a5.947 "
              @"5.947 0 00-5.827 4.757C1.713 5.447 0 7.945 0 10.49c0 1.666.713 3.283 1.998 "
              @"4.448-.119.5-.19 1-.19 1.499 0 3.401 2.759 5.946 5.946 5.946.642 0 1.26-.095 "
              @"1.88-.309a5.96 5.96 0 004.162 1.713z",
      },
      @"commandcode" : @{
        @"d" : @"m93.6604 26.1784c-8.982 0-16.2887 7.3067-16.2887 "
              @"16.2888v6.9809h-18.6158v-6.9809c0-8.9821-7.3067-16.2888-16.2887-16.2888-8.9821 0-16.2888"
              @" 7.3067-16.2888 16.2888s7.3067 16.2887 16.2888 16.2887h6.9809v18.6158h-6.9809c-8.9821 "
              @"0-16.2888 7.3067-16.2888 16.2888 0 8.9825 7.3067 16.2885 16.2888 16.2885 8.982 0 "
              @"16.2887-7.306 16.2887-16.2885v-6.981h18.6158v6.981c0 8.9825 7.3067 16.2885 16.2887 "
              @"16.2885 8.9826 0 16.2886-7.306 16.2886-16.2885 "
              @"0-8.9821-7.306-16.2888-16.2886-16.2888h-6.9809v-18.6158h6.9809c8.9826 0 16.2886-7.3066 "
              @"16.2886-16.2887s-7.306-16.2888-16.2886-16.2888zm-6.9809 23.2697v-6.9809c0-3.8628 "
              @"3.1182-6.9809 6.9809-6.9809 3.8628 0 6.9806 3.1181 6.9806 6.9809 0 3.8627-3.1178 "
              @"6.9809-6.9806 6.9809zm-44.2123 0c-3.8628 0-6.9809-3.1182-6.9809-6.9809 0-3.8628 "
              @"3.1181-6.9809 6.9809-6.9809 3.8627 0 6.9809 3.1181 6.9809 6.9809v6.9809zm16.2887 "
              @"27.9236v-18.6158h18.6158v18.6158zm34.9045 23.2693c-3.8627 "
              @"0-6.9809-3.1178-6.9809-6.9805v-6.981h6.9809c3.8628 0 6.9806 3.1182 6.9806 6.981 0 "
              @"3.8627-3.1178 6.9805-6.9806 6.9805zm-51.1932 0c-3.8628 0-6.9809-3.1178-6.9809-6.9805 "
              @"0-3.8628 3.1181-6.981 6.9809-6.981h6.9809v6.981c0 3.8627-3.1182 6.9805-6.9809 6.9805z",
        // Drawn for the inside of its app icon, so its strokes are far heavier
        // than the Mono marks beside it. Padding the box scales it down until
        // its ink area matches theirs; square, so a 17x17 draw does not stretch it.
        @"viewBox" : @"19.2 19.7 96.6 96.6",
      },
      @"claude" : @{
        @"d" : @"M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-"
              @".571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l"
              @".055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.15"
              @"8-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.07"
              @"3-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 "
              @"01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.2"
              @"43.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48"
              @".685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 "
              @"1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 "
              @"1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 "
              @"1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549."
              @"146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.3"
              @"29h-.182v.11l1.093 1.068 2.006 1.81 2.509 "
              @"2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345"
              @" 3.521.122 "
              @"1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.3"
              @"16.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042"
              @"-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 "
              @"2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 "
              @"18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z",
      }
    };
  });
  return paths[providerID];
}

// ---------- usage view ----------

- (void)clearHolder:(NSView *)holder {
  for (NSView *view in [holder.subviews copy]) {
    [view removeFromSuperview];
  }
}

/// Pin a freshly built header/footer row to every edge of its holder.
- (void)pinChrome:(NSView *)row inHolder:(NSView *)holder {
  row.translatesAutoresizingMaskIntoConstraints = NO;
  [holder addSubview:row];
  [NSLayoutConstraint activateConstraints:@[
    [row.leadingAnchor constraintEqualToAnchor:holder.leadingAnchor],
    [row.trailingAnchor constraintEqualToAnchor:holder.trailingAnchor],
    [row.topAnchor constraintEqualToAnchor:holder.topAnchor],
    [row.bottomAnchor constraintEqualToAnchor:holder.bottomAnchor],
  ]];
}

- (NSString *)titleForActiveProvider {
  NSString *active = self.state[@"active"] ?: @"";
  for (NSDictionary *provider in self.state[@"providers"]) {
    if ([provider[@"id"] isEqualToString:active]) {
      return provider[@"label"] ?: OCGT(@"Usage");
    }
  }
  return OCGT(@"Usage");
}

/// Add a row to the scrolling content area under the previous one.
- (void)addRow:(NSView *)row height:(CGFloat)height previous:(NSView **)previous topGap:(CGFloat)gap {
  OCGAddRowTo(self.content, row, height, previous, gap);
}

- (void)renderUsage {
  [self clearHolder:self.headerHolder];
  for (NSView *view in [self.content.subviews copy]) {
    [view removeFromSuperview];
  }

  NSString *active = self.state[@"active"] ?: @"";
  NSDictionary *result = self.state[@"results"][active] ?: @{};

  NSString *updatedAt = self.state[@"updated_at"];
  NSString *updatedText = OCGT(@"waiting for data");
  if ([updatedAt isKindOfClass:[NSString class]] && updatedAt.length > 0) {
    updatedText = updatedAt;
  }
  [self pinChrome:[self makeHeaderRowWithTitle:[self titleForActiveProvider] updated:updatedText]
         inHolder:self.headerHolder];

  NSView *previous = nil;
  NSString *error = result[@"error"];
  if ([error isKindOfClass:[NSString class]] && error.length > 0) {
    BOOL notConfigured = [error isEqualToString:OCGT(@"not configured")];
    NSString *displayText = notConfigured
        ? [NSString stringWithFormat:OCGT(@"%@ isn't set up yet."), [self titleForActiveProvider]]
        : error;
    NSTextField *errLabel = [self label:displayText size:kOCGFontCardTitle weight:NSFontWeightRegular color:OCGTextSecondary()];
    errLabel.maximumNumberOfLines = 0;
    [self addRow:errLabel height:32 previous:&previous topGap:12];
    if (notConfigured) {
      NSButton *configureButton =
          [NSButton buttonWithTitle:OCGT(@"OPEN SETTINGS") target:self action:@selector(toggleSettings:)];
      configureButton.translatesAutoresizingMaskIntoConstraints = NO;
      OCGStylePrimaryButton(configureButton, OCGT(@"OPEN SETTINGS"));
      [self addRow:configureButton height:26 previous:&previous topGap:10];
    }
  } else {
    NSArray *meters = result[@"meters"] ?: @[];
    NSView *card = nil;     // current account card
    NSView *cardLast = nil; // last row inside it
    for (NSDictionary *meter in meters) {
      NSString *key = meter[@"key"];
      NSString *group = meter[@"group"];
      BOOL grouped = [group isKindOfClass:[NSString class]] && group.length > 0;
      // One card per account. The heading alone cannot tell accounts apart:
      // one person in two ChatGPT workspaces has the same email on both.
      NSString *cardID = [key isKindOfClass:[NSString class]] && key.length > 0
                             ? key : group;
      if (grouped && (card == nil || ![cardID isEqualToString:card.identifier])) {
        if (card != nil && cardLast != nil) {
          [NSLayoutConstraint activateConstraints:@[
            [cardLast.bottomAnchor constraintEqualToAnchor:card.bottomAnchor constant:-8],
          ]];
        }
        NSString *selectedKey = self.state[@"selected_key"];
        BOOL isSelected = [selectedKey isKindOfClass:[NSString class]] &&
                           [selectedKey isEqualToString:key ?: @""];
        NSString *notice = self.state[@"notices"][key ?: @""];
        BOOL hasNotice = [notice isKindOfClass:[NSString class]] && notice.length > 0;
        BOOL busy = hasNotice && [notice hasPrefix:OCGT(@"Starting")];
        // Codex accounts Codex could be switched to: saved ones whose login
        // still works (a dead one would sign Codex out too).
        NSInteger switchState = 0;
        if ([active isEqualToString:@"codex"]) {
          for (NSDictionary *account in self.state[@"accounts"][@"codex"] ?: @[]) {
            if ([account[@"key"] isEqualToString:key ?: @""]) {
              NSString *problem = account[@"error"];
              BOOL usable = ![account[@"active"] boolValue] &&
                            !([problem isKindOfClass:[NSString class]] && problem.length > 0);
              if (usable) {
                switchState = [self.pendingSwitchKey isEqualToString:key] ? 2 : 1;
              }
              break;
            }
          }
        }
        card = [self cardViewWithTitle:group
                                 badge:meter[@"badge"]
                                   tag:meter[@"tag"]
                                   key:key
                              selected:isSelected
                             startable:[meter[@"can_start"] boolValue]
                                  busy:busy
                           switchState:switchState];
        card.identifier = cardID;
        cardLast = [card viewWithTag:1]; // the title label
        [self addCard:card previous:&previous];
        if (hasNotice) {
          BOOL calm = busy || [notice hasPrefix:OCGT(@"Codex now uses")];
          NSTextField *noticeLabel = [self label:notice size:kOCGFontDetail weight:NSFontWeightRegular
                                           color:(calm ? OCGTextSecondary() : OCGAccentAmber())];
          noticeLabel.lineBreakMode = NSLineBreakByTruncatingTail;
          noticeLabel.toolTip = notice;
          noticeLabel.translatesAutoresizingMaskIntoConstraints = NO;
          [card addSubview:noticeLabel];
          [NSLayoutConstraint activateConstraints:@[
            [noticeLabel.leadingAnchor constraintEqualToAnchor:card.leadingAnchor constant:11],
            [noticeLabel.trailingAnchor constraintEqualToAnchor:card.trailingAnchor constant:-11],
            [noticeLabel.topAnchor constraintEqualToAnchor:cardLast.bottomAnchor constant:3],
            [noticeLabel.heightAnchor constraintEqualToConstant:15],
          ]];
          cardLast = noticeLabel;
        }
      }
      if (grouped) {
        NSView *row = [self compactMeterRow:meter];
        row.translatesAutoresizingMaskIntoConstraints = NO;
        [card addSubview:row];
        // Count/balance rows carry no bar, so they need only their text line.
        CGFloat rowHeight = [meter[@"informational"] boolValue] ? 17 : 24;
        [NSLayoutConstraint activateConstraints:@[
          [row.leadingAnchor constraintEqualToAnchor:card.leadingAnchor constant:11],
          [row.trailingAnchor constraintEqualToAnchor:card.trailingAnchor constant:-11],
          [row.topAnchor constraintEqualToAnchor:cardLast.bottomAnchor constant:4],
          [row.heightAnchor constraintEqualToConstant:rowHeight],
        ]];
        cardLast = row;
      } else {
        CGFloat rowHeight = [meter[@"informational"] boolValue] ? 30 : 48;
        [self addRow:[self meterRow:meter] height:rowHeight previous:&previous topGap:5];
        card = nil;
      }
    }
    if (card != nil && cardLast != nil) {
      [NSLayoutConstraint activateConstraints:@[
        [cardLast.bottomAnchor constraintEqualToAnchor:card.bottomAnchor constant:-8],
      ]];
    }
  }

  if (previous != nil) {
    [NSLayoutConstraint activateConstraints:@[
      [previous.bottomAnchor constraintEqualToAnchor:self.content.bottomAnchor constant:-10],
    ]];
  }
}

/// Add a card to the scrolling content: width pinned, height from its contents.
- (void)addCard:(NSView *)card previous:(NSView **)previous {
  card.translatesAutoresizingMaskIntoConstraints = NO;
  [self.content addSubview:card];
  NSMutableArray *constraints = [NSMutableArray array];
  [constraints addObject:[card.leadingAnchor constraintEqualToAnchor:self.content.leadingAnchor constant:12]];
  [constraints addObject:[card.trailingAnchor constraintEqualToAnchor:self.content.trailingAnchor constant:-12]];
  if (*previous == nil) {
    [constraints addObject:[card.topAnchor constraintEqualToAnchor:self.content.topAnchor constant:2]];
  } else {
    [constraints addObject:[card.topAnchor constraintEqualToAnchor:(*previous).bottomAnchor constant:7]];
  }
  [NSLayoutConstraint activateConstraints:constraints];
  *previous = card;
}

/// "/Users/x/.codex2" -> "~/.codex2". A card key that is not a path (the
/// single-login providers use a fixed id) has nothing worth showing here.
- (NSString *)abbreviateHome:(NSString *)path {
  if (![path isKindOfClass:[NSString class]] || path.length == 0 ||
      !([path hasPrefix:@"/"] || [path hasPrefix:@"~"])) {
    return @"";
  }
  NSString *home = NSHomeDirectory();
  if ([path hasPrefix:home]) {
    return [@"~" stringByAppendingString:[path substringFromIndex:home.length]];
  }
  return path;
}

/// One card per account: rounded panel, email/plan on the left, the CODEX_HOME
/// on the right — so several logins never blur into one list. Click anywhere
/// on the card to pin it as the one that drives the menu bar badge (click
/// the pinned one again to un-pin); a green border + pin glyph mark whichever
/// is currently pinned.
- (NSView *)cardViewWithTitle:(NSString *)title
                        badge:(NSString *)badge
                          tag:(NSString *)tag
                          key:(NSString *)key
                     selected:(BOOL)selected
                    startable:(BOOL)startable
                         busy:(BOOL)busy
                  switchState:(NSInteger)switchState {
  // A raised card on the dark surface: a visible border (not just a tint)
  // now that the background itself is no longer near-black-on-near-black.
  OCGCardView *card = [[OCGCardView alloc] initWithFrame:NSZeroRect];
  card.wantsLayer = YES;
  card.layer.cornerRadius = 8;
  card.layer.backgroundColor = OCGCardColor().CGColor;
  // The pin glyph already says which card is selected, so its frame only has
  // to whisper: a hairline of dimmed accent rather than a bright full-strength
  // outline, which at full width read as a warning box.
  // The pin glyph already says which card is selected, so its frame only has
  // to whisper. Dimming the accent with alpha muddied it — on a dark card that
  // blends toward the card colour, losing saturation as well as brightness —
  // so this is a deeper green at full strength instead, which holds up at a
  // single pixel wide without glaring.
  card.layer.borderWidth = selected ? 0.5 : 1;
  card.layer.borderColor = (selected ? OCGSelectedBorderColor() : OCGBorderColor()).CGColor;

  NSTextField *titleLabel = [self label:title size:kOCGFontCardTitle weight:NSFontWeightSemibold color:OCGTextPrimary()];
  titleLabel.tag = 1;
  titleLabel.toolTip = title;
  [titleLabel setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                       forOrientation:NSLayoutConstraintOrientationHorizontal];
  [card addSubview:titleLabel];

  // A quiet note at the right of the header — "in Codex" on the account the
  // Codex CLI uses — or, for a card keyed by a path, that path.
  //
  // On an account Codex could be switched to, that slot is the switch itself:
  // "Use", then "Sure?" for the second press. Switching rewrites the login
  // the Codex CLI reads, so it takes two presses — a sheet cannot confirm it
  // here, since a popover closes when another window takes focus.
  BOOL hasTag = [tag isKindOfClass:[NSString class]] && tag.length > 0;
  NSView *homeLabel;
  if (switchState > 0) {
    BOOL confirming = switchState == 2;
    NSButton *use = [NSButton buttonWithTitle:@"" target:self action:@selector(switchTagClicked:)];
    use.identifier = key;
    use.bordered = NO;
    use.attributedTitle = [[NSAttributedString alloc]
        initWithString:(confirming ? OCGT(@"Sure?") : OCGT(@"Use"))
            attributes:@{
              NSFontAttributeName : OCGMonoFont(kOCGFontDetail, confirming ? NSFontWeightSemibold : NSFontWeightRegular),
              NSForegroundColorAttributeName : confirming ? OCGAccentAmber() : OCGTextSecondary(),
            }];
    use.toolTip = confirming
        ? OCGT(@"Click again to sign Codex in to this account")
        : OCGT(@"Use this account in Codex — the one it uses now is kept here instead");
    use.translatesAutoresizingMaskIntoConstraints = NO;
    homeLabel = use;
  } else {
    NSString *home = hasTag ? tag : [self abbreviateHome:key];
    NSTextField *label = [self label:home size:kOCGFontDetail weight:NSFontWeightRegular
                               color:(hasTag ? OCGAccentGreen() : OCGTextTertiary())];
    label.toolTip = hasTag ? OCGT(@"The account the Codex CLI is signed in to") : key;
    homeLabel = label;
  }
  [card addSubview:homeLabel];

  // "Start the 5h window now": the window only begins with the first real
  // request, so this lets the user pick when it ends. Sits just before the
  // home path; OCGCardView lets its clicks through instead of selecting.
  NSButton *start = nil;
  if (startable) {
    start = [NSButton buttonWithTitle:@"" target:self action:@selector(startWindowClicked:)];
    start.identifier = key;
    start.image = [NSImage imageWithSystemSymbolName:(busy ? @"hourglass" : @"play.circle")
                           accessibilityDescription:OCGT(@"Start 5h window")];
    start.imagePosition = NSImageOnly;
    start.bordered = NO;
    start.enabled = !busy;
    start.contentTintColor = busy ? OCGTextTertiary() : OCGAccentGreen();
    start.toolTip = busy ? OCGT(@"Starting the 5h window…")
                         : OCGT(@"Start the 5h window now (sends one tiny request)");
    start.translatesAutoresizingMaskIntoConstraints = NO;
    [card addSubview:start];
  }

  NSImageView *pin = nil;
  if (selected) {
    pin = [[NSImageView alloc] initWithFrame:NSZeroRect];
    pin.image = [NSImage imageWithSystemSymbolName:@"pin.fill" accessibilityDescription:OCGT(@"Pinned")];
    pin.contentTintColor = OCGAccentGreen();
    pin.toolTip = OCGT(@"Driving the menu bar badge — click to un-pin");
    pin.translatesAutoresizingMaskIntoConstraints = NO;
    [card addSubview:pin];
  }

  // The pin sits before the title when present, so the title's own leading
  // anchor targets whichever comes right before it — never both at once.
  NSLayoutConstraint *titleLeading = pin != nil
      ? [titleLabel.leadingAnchor constraintEqualToAnchor:pin.trailingAnchor constant:4]
      : [titleLabel.leadingAnchor constraintEqualToAnchor:card.leadingAnchor constant:11];
  [NSLayoutConstraint activateConstraints:@[
    titleLeading,
    [titleLabel.topAnchor constraintEqualToAnchor:card.topAnchor constant:8],
    [homeLabel.trailingAnchor constraintEqualToAnchor:card.trailingAnchor constant:-11],
    [homeLabel.firstBaselineAnchor constraintEqualToAnchor:titleLabel.firstBaselineAnchor],
  ]];
  if (start != nil) {
    [NSLayoutConstraint activateConstraints:@[
      [start.trailingAnchor constraintEqualToAnchor:homeLabel.leadingAnchor constant:-5],
      [start.centerYAnchor constraintEqualToAnchor:titleLabel.centerYAnchor],
      [start.widthAnchor constraintEqualToConstant:18],
      [start.heightAnchor constraintEqualToConstant:18],
    ]];
  }
  // Whatever sits left of the home path — the start button if there is one.
  NSLayoutXAxisAnchor *headerRightEdge = start != nil ? start.leadingAnchor : homeLabel.leadingAnchor;
  if (pin != nil) {
    [NSLayoutConstraint activateConstraints:@[
      [pin.leadingAnchor constraintEqualToAnchor:card.leadingAnchor constant:11],
      [pin.centerYAnchor constraintEqualToAnchor:titleLabel.centerYAnchor],
      [pin.widthAnchor constraintEqualToConstant:11],
      [pin.heightAnchor constraintEqualToConstant:11],
    ]];
  }

  // 计划名徽标：跟在标题后，空间不足时最先让位。
  if ([badge isKindOfClass:[NSString class]] && badge.length > 0) {
    NSTextField *badgeLabel = [self label:badge size:kOCGFontBadge weight:NSFontWeightSemibold color:OCGTextSecondary()];
    badgeLabel.wantsLayer = YES;
    badgeLabel.layer.cornerRadius = 3;
    badgeLabel.layer.backgroundColor = OCGTrackColor().CGColor;
    [badgeLabel.heightAnchor constraintEqualToConstant:14].active = YES;
    [card addSubview:badgeLabel];
    [NSLayoutConstraint activateConstraints:@[
      [badgeLabel.leadingAnchor constraintGreaterThanOrEqualToAnchor:titleLabel.trailingAnchor
                                                            constant:5],
      [badgeLabel.trailingAnchor constraintLessThanOrEqualToAnchor:headerRightEdge constant:-4],
      [badgeLabel.firstBaselineAnchor constraintEqualToAnchor:titleLabel.firstBaselineAnchor],
    ]];
  }

  // An invisible button covering the card takes the click; OCGCardView's
  // -hitTest: routes every point inside the card to it, so the meter rows
  // the caller appends afterwards cannot shadow it. A card with no key is
  // not selectable and gets no hit area at all.
  if (key.length > 0) {
    NSButton *hitArea = [NSButton buttonWithTitle:@"" target:self action:@selector(cardClicked:)];
    hitArea.identifier = key;
    hitArea.bordered = NO;
    hitArea.title = @"";
    // The card's own hit-testing now owns every tooltip inside it too, so
    // this one carries what the title and path labels used to show.
    hitArea.toolTip = [NSString stringWithFormat:@"%@\n%@\n%@", title, key,
                                                 selected ? OCGT(@"Driving the menu bar badge — click to un-pin")
                                                          : OCGT(@"Click to pin to the menu bar badge")];
    hitArea.translatesAutoresizingMaskIntoConstraints = NO;
    [card addSubview:hitArea];
    card.hitArea = hitArea;
    [NSLayoutConstraint activateConstraints:@[
      [hitArea.leadingAnchor constraintEqualToAnchor:card.leadingAnchor],
      [hitArea.trailingAnchor constraintEqualToAnchor:card.trailingAnchor],
      [hitArea.topAnchor constraintEqualToAnchor:card.topAnchor],
      [hitArea.bottomAnchor constraintEqualToAnchor:card.bottomAnchor],
    ]];
  }
  return card;
}

- (void)cardClicked:(NSButton *)sender {
  NSString *key = sender.identifier ?: @"";
  if (key.length == 0) {
    return;
  }
  NSString *active = self.state[@"active"] ?: @"";
  NSString *selectedKey = self.state[@"selected_key"];
  BOOL alreadySelected = [selectedKey isKindOfClass:[NSString class]] && [selectedKey isEqualToString:key];
  goSetSelectedAccount(active.UTF8String, alreadySelected ? "" : key.UTF8String);

  // Adopt the new pin locally instead of waiting for the answer: the Rust
  // side replies asynchronously (config write, then a full state push), and
  // until that lands every further click still reads the *old* selection —
  // so a second click on a just-pinned card re-pins it rather than un-pinning
  // it, and looks like the click did nothing.
  NSMutableDictionary *optimistic = [self.state mutableCopy];
  if (alreadySelected) {
    [optimistic removeObjectForKey:@"selected_key"];
  } else {
    optimistic[@"selected_key"] = key;
  }
  self.state = optimistic;
  // Re-render on the next turn: this click's button is about to be destroyed
  // by the rebuild, and the cell is still finishing its tracking right now.
  dispatch_async(dispatch_get_main_queue(), ^{
    [self renderAll];
  });
}

/// First press arms the switch and shows "Sure?"; a second press on the same
/// account within three seconds does it. Otherwise it quietly disarms.
- (void)switchTagClicked:(NSButton *)sender {
  NSString *key = sender.identifier ?: @"";
  if (key.length == 0) {
    return;
  }
  if ([self.pendingSwitchKey isEqualToString:key]) {
    self.pendingSwitchKey = nil;
    goCodexUseAccount(key.UTF8String);
    // Next turn: this button is about to be rebuilt, and is still mid-click.
    dispatch_async(dispatch_get_main_queue(), ^{
      [self renderAll];
    });
    return;
  }
  self.pendingSwitchKey = key;
  dispatch_async(dispatch_get_main_queue(), ^{
    [self renderAll];
  });
  dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(3 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
    if ([self.pendingSwitchKey isEqualToString:key]) {
      self.pendingSwitchKey = nil;
      [self renderAll];
    }
  });
}

- (void)startWindowClicked:(NSButton *)sender {
  NSString *key = sender.identifier ?: @"";
  if (key.length == 0) {
    return;
  }
  NSString *active = self.state[@"active"] ?: @"";
  goStartWindow(active.UTF8String, key.UTF8String);
}

/// Thin rounded meter bar. NSProgressIndicator cannot be tinted, so the fill is
/// a plain layer whose width tracks the percentage. An exhausted quota (0% left)
/// tints the empty track red and lets it breathe slowly — the one case that gets
/// a little motion, so "almost empty" and "gone" never look the same.
- (NSView *)barViewForMeter:(NSDictionary *)meter percent:(double)percent {
  NSColor *tint = [self severityColor:meter];
  BOOL exhausted = NO;
  NSNumber *severity = meter[@"severity"];
  if ([severity isKindOfClass:[NSNumber class]] && severity.intValue >= 100) {
    exhausted = YES;
  }
  NSView *track = [[NSView alloc] initWithFrame:NSZeroRect];
  track.wantsLayer = YES;
  track.layer.cornerRadius = 2.5;
  track.layer.backgroundColor = OCGTrackColor().CGColor;
  if (exhausted) {
    track.layer.backgroundColor = [OCGAccentRed() colorWithAlphaComponent:0.32].CGColor;
    CABasicAnimation *breathe = [CABasicAnimation animationWithKeyPath:@"opacity"];
    breathe.fromValue = @0.35;
    breathe.toValue = @1.0;
    breathe.duration = 1.8;
    breathe.autoreverses = YES;
    breathe.repeatCount = HUGE_VALF;
    breathe.timingFunction =
        [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionEaseInEaseOut];
    [track.layer addAnimation:breathe forKey:@"ocg-breathe"];
  }

  NSView *fill = [[NSView alloc] initWithFrame:NSZeroRect];
  fill.wantsLayer = YES;
  fill.layer.cornerRadius = 2.5;
  // Inert rows (credits) get a neutral fill; quota rows are always coloured.
  fill.layer.backgroundColor = (tint ?: OCGTextSecondary()).CGColor;
  fill.translatesAutoresizingMaskIntoConstraints = NO;
  [track addSubview:fill];
  [NSLayoutConstraint activateConstraints:@[
    [fill.leadingAnchor constraintEqualToAnchor:track.leadingAnchor],
    [fill.topAnchor constraintEqualToAnchor:track.topAnchor],
    [fill.bottomAnchor constraintEqualToAnchor:track.bottomAnchor],
  ]];
  if (percent > 0.5) {
    NSLayoutConstraint *width = [NSLayoutConstraint constraintWithItem:fill
                                                            attribute:NSLayoutAttributeWidth
                                                            relatedBy:NSLayoutRelationEqual
                                                               toItem:track
                                                           attribute:NSLayoutAttributeWidth
                                                          multiplier:MIN(1.0, MAX(0.02, percent / 100.0))
                                                             constant:0];
    width.active = YES;
  }
  return track;
}

/// Bar fill: the full three-state palette (green while there is room).
- (NSColor *)severityColor:(NSDictionary *)meter {
  NSNumber *severity = meter[@"severity"];
  if (![severity isKindOfClass:[NSNumber class]]) {
    return nil;
  }
  return OCGStatusColorFixed(severity.intValue);
}

/// Text colour: neutral while healthy, so only problems are coloured.
- (NSColor *)severityTextColor:(NSDictionary *)meter {
  NSNumber *severity = meter[@"severity"];
  if (![severity isKindOfClass:[NSNumber class]]) {
    return nil;
  }
  int used = severity.intValue;
  if (used > 90) {
    return OCGAccentRed();
  }
  if (used > 70) {
    return OCGAccentAmber();
  }
  return nil; // falls back to OCGTextPrimary()
}

/// Compact meter for use inside a card: one text line over a thin bar.
- (NSView *)compactMeterRow:(NSDictionary *)meter {
  NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];

  NSString *labelText = meter[@"label"] ?: OCGT(@"Usage");
  NSNumber *percentNumber = meter[@"percent"] ?: @0;
  NSString *detailText = meter[@"detail"] ?: @"";
  double percent = MAX(0, MIN(100, percentNumber.doubleValue));
  BOOL informational = [meter[@"informational"] boolValue];

  NSString *labelString = informational
      ? labelText
      : [NSString stringWithFormat:@"%@  %.0f%%", labelText, percent];
  NSTextField *label = [self label:labelString
                              size:kOCGFontBody weight:NSFontWeightSemibold
                             color:([self severityTextColor:meter] ?: OCGTextPrimary())];
  // The label carries the headline (window + percentage) and must never lose
  // it; when the row is too narrow the detail on the right gives way instead.
  [label setContentCompressionResistancePriority:NSLayoutPriorityDefaultHigh
                                  forOrientation:NSLayoutConstraintOrientationHorizontal];
  NSTextField *detail =
      [self label:detailText size:kOCGFontDetail weight:NSFontWeightRegular color:OCGTextSecondary()];
  detail.toolTip = detailText;
  [detail setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                   forOrientation:NSLayoutConstraintOrientationHorizontal];

  label.translatesAutoresizingMaskIntoConstraints = NO;
  detail.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:label];
  [row addSubview:detail];

  [NSLayoutConstraint activateConstraints:@[
    [label.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
    [label.topAnchor constraintEqualToAnchor:row.topAnchor],
    [detail.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
    [detail.firstBaselineAnchor constraintEqualToAnchor:label.firstBaselineAnchor],
    [detail.leadingAnchor constraintGreaterThanOrEqualToAnchor:label.trailingAnchor constant:6],
  ]];

  // A balance or a count has no percentage behind it, so a bar drawn from the
  // placeholder percent would just read as an empty (depleted) quota.
  if (!informational) {
    NSView *bar = [self barViewForMeter:meter percent:percent];
    bar.translatesAutoresizingMaskIntoConstraints = NO;
    [row addSubview:bar];
    [NSLayoutConstraint activateConstraints:@[
      [bar.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
      [bar.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
      [bar.bottomAnchor constraintEqualToAnchor:row.bottomAnchor],
      [bar.heightAnchor constraintEqualToConstant:6],
    ]];
  }
  return row;
}

- (NSView *)meterRow:(NSDictionary *)meter {
  NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];
  // No fill — flat, linear, separated by whitespace alone.

  NSString *labelText = meter[@"label"] ?: OCGT(@"Usage");
  NSNumber *percentNumber = meter[@"percent"] ?: @0;
  NSString *detailText = meter[@"detail"] ?: @"";
  double percent = MAX(0, MIN(100, percentNumber.doubleValue));
  BOOL informational = [meter[@"informational"] boolValue];

  NSString *labelString = informational
      ? labelText
      : [NSString stringWithFormat:@"%@  %.0f%%", labelText, percent];
  NSTextField *label = [self label:labelString
                              size:kOCGFontCardTitle weight:NSFontWeightSemibold
                             color:([self severityTextColor:meter] ?: OCGTextPrimary())];
  NSTextField *detail = [self label:detailText size:kOCGFontBody weight:NSFontWeightRegular color:OCGTextSecondary()];
  [detail setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                   forOrientation:NSLayoutConstraintOrientationHorizontal];
  label.translatesAutoresizingMaskIntoConstraints = NO;
  detail.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:label];
  [row addSubview:detail];

  [NSLayoutConstraint activateConstraints:@[
    [label.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
    [label.topAnchor constraintEqualToAnchor:row.topAnchor constant:8],
    [detail.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
    [detail.centerYAnchor constraintEqualToAnchor:label.centerYAnchor],
    [detail.leadingAnchor constraintGreaterThanOrEqualToAnchor:label.trailingAnchor constant:8],
  ]];

  // A balance or a count has no percentage behind it, so a bar drawn from the
  // placeholder percent would just read as an empty (depleted) quota.
  if (!informational) {
    NSView *bar = [self barViewForMeter:meter percent:percent];
    bar.translatesAutoresizingMaskIntoConstraints = NO;
    [row addSubview:bar];
    [NSLayoutConstraint activateConstraints:@[
      [bar.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
      [bar.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
      [bar.topAnchor constraintEqualToAnchor:label.bottomAnchor constant:7],
      [bar.heightAnchor constraintEqualToConstant:9],
    ]];
  }
  return row;
}

// ---------- shared subviews ----------

- (NSView *)makeHeaderRowWithTitle:(NSString *)titleText updated:(NSString *)updatedText {
  NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];
  row.translatesAutoresizingMaskIntoConstraints = NO;

  NSTextField *title = [self label:titleText size:kOCGFontTitle weight:NSFontWeightBold color:OCGTextPrimary()];
  title.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:title];

  // Last refresh time, right after the name — it used to be its own row
  // above the cards, which cost a line for a detail nobody reads twice.
  NSTextField *updated = [self label:updatedText size:kOCGFontDetail weight:NSFontWeightRegular color:OCGTextTertiary()];
  updated.toolTip = OCGT(@"Last refreshed");
  [updated setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                    forOrientation:NSLayoutConstraintOrientationHorizontal];
  updated.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:updated];

  // A quick refresh right next to Preferences — always in view, no more
  // hunting for a footer button below a tall account list.
  NSButton *refresh =
      [NSButton buttonWithTitle:@"" target:self action:@selector(refreshClicked:)];
  refresh.image = [NSImage imageWithSystemSymbolName:@"arrow.clockwise"
                             accessibilityDescription:OCGT(@"Refresh")];
  refresh.imagePosition = NSImageOnly;
  refresh.bordered = NO;
  refresh.contentTintColor = OCGTextSecondary();
  refresh.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:refresh];

  // Opens the standalone Preferences window — the popover only ever shows
  // the usage view now, so this is always a gear, never a back chevron.
  NSButton *gear =
      [NSButton buttonWithTitle:@"" target:self action:@selector(toggleSettings:)];
  gear.image = [NSImage imageWithSystemSymbolName:@"gearshape" accessibilityDescription:OCGT(@"Preferences")];
  gear.imagePosition = NSImageOnly;
  gear.bordered = NO;
  gear.contentTintColor = OCGTextSecondary();
  gear.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:gear];

  // Quit lives here now, as a top-right close button — no footer, no
  // separate Quit row; the corner "×" is the standard place to look for it.
  NSButton *close =
      [NSButton buttonWithTitle:@"" target:self action:@selector(quitClicked:)];
  close.image = [NSImage imageWithSystemSymbolName:@"xmark" accessibilityDescription:OCGT(@"Quit")];
  close.imagePosition = NSImageOnly;
  close.bordered = NO;
  close.contentTintColor = OCGTextSecondary();
  close.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:close];

  [NSLayoutConstraint activateConstraints:@[
    [title.leadingAnchor constraintEqualToAnchor:row.leadingAnchor constant:16],
    [title.topAnchor constraintEqualToAnchor:row.topAnchor constant:10],
    [updated.leadingAnchor constraintEqualToAnchor:title.trailingAnchor constant:8],
    [updated.firstBaselineAnchor constraintEqualToAnchor:title.firstBaselineAnchor],
    [updated.trailingAnchor constraintLessThanOrEqualToAnchor:refresh.leadingAnchor constant:-6],
    [close.trailingAnchor constraintEqualToAnchor:row.trailingAnchor constant:-12],
    [close.centerYAnchor constraintEqualToAnchor:title.centerYAnchor],
    [close.widthAnchor constraintEqualToConstant:27],
    [close.heightAnchor constraintEqualToConstant:27],
    [gear.trailingAnchor constraintEqualToAnchor:close.leadingAnchor constant:-4],
    [gear.centerYAnchor constraintEqualToAnchor:title.centerYAnchor],
    [gear.widthAnchor constraintEqualToConstant:27],
    [gear.heightAnchor constraintEqualToConstant:27],
    [refresh.trailingAnchor constraintEqualToAnchor:gear.leadingAnchor constant:-4],
    [refresh.centerYAnchor constraintEqualToAnchor:title.centerYAnchor],
    [refresh.widthAnchor constraintEqualToConstant:27],
    [refresh.heightAnchor constraintEqualToConstant:27],
  ]];
  return row;
}

- (NSTextField *)label:(NSString *)text size:(CGFloat)size weight:(NSFontWeight)weight color:(NSColor *)color {
  return OCGLabel(text, size, weight, color);
}

// ---------- actions ----------

- (void)providerClicked:(NSButton *)sender {
  NSString *provider = sender.identifier;
  if (provider.length == 0) {
    return;
  }
  goProviderSelected(provider.UTF8String);
}

/// Settings lives in its own Preferences window now — ask the app delegate
/// to show it (and close this transient popover) instead of swapping views.
- (void)toggleSettings:(id)sender {
  [self.appDelegate showPreferencesWindow];
}

- (void)refreshClicked:(id)sender {
  goRefreshRequested();
}

- (void)quitClicked:(id)sender {
  goQuitRequested();
  [NSApp terminate:nil];
}

@end

// ---------------------------------------------------------------------------
// SettingsWindowController: the standalone Preferences window.
//
// Organised by provider: a sidebar lists General and each provider; a
// provider's page holds everything about it — whether it is on, its accounts
// (the CLI's own login, accounts signed in from here, an API key), how to add
// one, and its own display options. Every control commits immediately, so
// there is no Save button and nothing here is ever "unsaved".
// ---------------------------------------------------------------------------

static NSString *const OCGGeneralPage = @"general";

/// A compact text button for a row action — no border, so a list of accounts
/// does not turn into a wall of boxes.
static NSButton *OCGLinkButton(NSString *title, NSColor *color, id target, SEL action) {
  NSButton *b = [NSButton buttonWithTitle:title target:target action:action];
  b.bordered = NO;
  b.attributedTitle = [[NSAttributedString alloc]
      initWithString:title
          attributes:@{
            NSFontAttributeName : OCGMonoFont(kOCGFontDetail, NSFontWeightSemibold),
            NSForegroundColorAttributeName : color,
          }];
  b.translatesAutoresizingMaskIntoConstraints = NO;
  return b;
}

/// Pin `row` across the settings content (24pt margins), under `*previous`.
static void OCGAddSettingsRow(NSView *content, NSView *row, CGFloat height, NSView **previous, CGFloat gap) {
  row.translatesAutoresizingMaskIntoConstraints = NO;
  [content addSubview:row];
  NSMutableArray *c = [NSMutableArray arrayWithArray:@[
    [row.leadingAnchor constraintEqualToAnchor:content.leadingAnchor constant:24],
    [row.trailingAnchor constraintEqualToAnchor:content.trailingAnchor constant:-24],
    [row.heightAnchor constraintEqualToConstant:height],
  ]];
  [c addObject:(*previous == nil ? [row.topAnchor constraintEqualToAnchor:content.topAnchor constant:22]
                                 : [row.topAnchor constraintEqualToAnchor:(*previous).bottomAnchor constant:gap])];
  [NSLayoutConstraint activateConstraints:c];
  *previous = row;
}

/// One line on what a provider's page is about and how it signs in.
static NSString *OCGProviderBlurb(NSString *provider) {
  NSDictionary *blurbs = @{
    @"opencode" : OCGT(@"OpenCode Go windows. Sign in with your OpenCode console account, or use an API key."),
    @"deepseek" : OCGT(@"Account balance, read with an API key from platform.deepseek.com."),
    @"minimax" : OCGT(@"Token Plan windows, read with the plan's Subscription Key."),
    @"codex" : OCGT(@"ChatGPT subscription windows for Codex, for every account you sign in."),
    @"commandcode" : OCGT(@"commandcode.ai credits and 5h / weekly windows."),
    @"claude" : OCGT(@"claude.ai subscription windows — Claude Code's login, and any you add."),
  };
  return blurbs[provider] ?: @"";
}

/// How an account came to be here, for its row.
static NSString *OCGSourceLabel(NSString *provider, NSDictionary *account) {
  NSString *source = account[@"source"] ?: @"";
  if ([source isEqualToString:@"key"]) {
    return OCGT(@"the key set below");
  }
  if ([source isEqualToString:@"cli"]) {
    if ([provider isEqualToString:@"codex"]) return OCGT(@"in Codex");
    if ([provider isEqualToString:@"claude"]) return OCGT(@"Claude Code's login");
    return OCGT(@"the CLI's login");
  }
  return OCGT(@"signed in here");
}

@interface SettingsWindowController : NSViewController <NSTextFieldDelegate>
@property(strong) NSView *sidebar;
@property(strong) NSView *content;
@property(strong) NSScrollView *scrollView;
@property(strong) NSDictionary *state;
/// "general", or a provider id.
@property(copy) NSString *selectedPage;
/// A field is mid-edit: skip re-rendering on a background state push so a
/// refresh triggered by some OTHER control doesn't wipe out what the user is
/// typing before they tab or click away.
@property BOOL editingText;
- (void)updateWithState:(NSDictionary *)state;
/// Show General ("general") or a provider's page.
- (void)showPage:(NSString *)page;
@end

@implementation SettingsWindowController

- (instancetype)init {
  self = [super initWithNibName:nil bundle:nil];
  if (self) {
    _selectedPage = OCGGeneralPage;
  }
  return self;
}

- (void)loadView {
  self.view = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 720, 540)];
  self.view.wantsLayer = YES;
  self.view.layer.backgroundColor = OCGPanelColor().CGColor;
  // The window is not user-resizable, so a fixed size here is safe — and
  // necessary: without it a shorter page could shrink the whole window.
  [self.view.widthAnchor constraintEqualToConstant:720].active = YES;
  [self.view.heightAnchor constraintEqualToConstant:540].active = YES;

  self.sidebar = [[OCGFlipView alloc] initWithFrame:NSZeroRect];
  self.sidebar.translatesAutoresizingMaskIntoConstraints = NO;
  self.sidebar.wantsLayer = YES;
  self.sidebar.layer.backgroundColor = OCGSurfaceInsetColor().CGColor;
  [self.view addSubview:self.sidebar];

  NSView *divider = [[NSView alloc] initWithFrame:NSZeroRect];
  divider.translatesAutoresizingMaskIntoConstraints = NO;
  divider.wantsLayer = YES;
  divider.layer.backgroundColor = OCGBorderColor().CGColor;
  [self.view addSubview:divider];

  self.scrollView = [[NSScrollView alloc] initWithFrame:NSZeroRect];
  self.scrollView.translatesAutoresizingMaskIntoConstraints = NO;
  self.scrollView.drawsBackground = NO;
  self.scrollView.hasVerticalScroller = YES;
  self.scrollView.autohidesScrollers = YES;
  self.scrollView.borderType = NSNoBorder;
  [self.view addSubview:self.scrollView];

  self.content = [[OCGFlipView alloc] initWithFrame:NSZeroRect];
  self.content.translatesAutoresizingMaskIntoConstraints = NO;
  self.scrollView.documentView = self.content;
  [self.content.widthAnchor constraintEqualToAnchor:self.scrollView.contentView.widthAnchor].active = YES;

  [NSLayoutConstraint activateConstraints:@[
    [self.sidebar.leadingAnchor constraintEqualToAnchor:self.view.leadingAnchor],
    [self.sidebar.topAnchor constraintEqualToAnchor:self.view.topAnchor],
    [self.sidebar.bottomAnchor constraintEqualToAnchor:self.view.bottomAnchor],
    [self.sidebar.widthAnchor constraintEqualToConstant:188],

    [divider.leadingAnchor constraintEqualToAnchor:self.sidebar.trailingAnchor],
    [divider.topAnchor constraintEqualToAnchor:self.view.topAnchor],
    [divider.bottomAnchor constraintEqualToAnchor:self.view.bottomAnchor],
    [divider.widthAnchor constraintEqualToConstant:1],

    [self.scrollView.leadingAnchor constraintEqualToAnchor:divider.trailingAnchor],
    [self.scrollView.trailingAnchor constraintEqualToAnchor:self.view.trailingAnchor],
    [self.scrollView.topAnchor constraintEqualToAnchor:self.view.topAnchor],
    [self.scrollView.bottomAnchor constraintEqualToAnchor:self.view.bottomAnchor],

    [self.content.topAnchor constraintEqualToAnchor:self.scrollView.contentView.topAnchor],
    [self.content.leadingAnchor constraintEqualToAnchor:self.scrollView.contentView.leadingAnchor],
    [self.content.trailingAnchor constraintEqualToAnchor:self.scrollView.contentView.trailingAnchor],
  ]];
}

- (void)viewWillAppear {
  [super viewWillAppear];
  [self renderAll];
}

- (void)updateWithState:(NSDictionary *)state {
  self.state = state;
  if (self.viewIfLoaded != nil && !self.editingText) {
    [self renderAll];
  }
}

- (void)showPage:(NSString *)page {
  self.selectedPage = page.length > 0 ? page : OCGGeneralPage;
  if (self.viewIfLoaded != nil) {
    [self renderAll];
  }
}

- (void)renderAll {
  [self renderSidebar];
  [self renderContent];
}

- (NSDictionary *)providerInfo:(NSString *)providerID {
  for (NSDictionary *p in self.state[@"providers"] ?: @[]) {
    if ([p[@"id"] isEqualToString:providerID]) {
      return p;
    }
  }
  return nil;
}

// ---------- sidebar ----------

- (NSButton *)sidebarRow:(NSString *)pageID title:(NSString *)title icon:(NSImage *)icon enabled:(BOOL)enabled {
  BOOL selected = [self.selectedPage isEqualToString:pageID];
  NSButton *row = [NSButton buttonWithTitle:@"" target:self action:@selector(pageClicked:)];
  row.identifier = pageID;
  row.bordered = NO;
  row.wantsLayer = YES;
  row.layer.cornerRadius = 6;
  row.layer.backgroundColor = selected ? OCGBorderColor().CGColor : NSColor.clearColor.CGColor;
  row.translatesAutoresizingMaskIntoConstraints = NO;

  NSImageView *iconView = [[NSImageView alloc] initWithFrame:NSZeroRect];
  iconView.image = icon;
  iconView.contentTintColor = selected ? OCGAccentGreen() : OCGTextSecondary();
  iconView.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:iconView];

  NSTextField *label = OCGLabel(title, kOCGFontBody, selected ? NSFontWeightSemibold : NSFontWeightRegular,
                                selected ? OCGTextPrimary() : OCGTextSecondary());
  [row addSubview:label];

  // A provider that is switched off is dimmed, so the list reads at a glance.
  NSView *dot = [[NSView alloc] initWithFrame:NSZeroRect];
  dot.wantsLayer = YES;
  dot.layer.cornerRadius = 3;
  dot.layer.backgroundColor = (enabled ? OCGAccentGreen() : OCGTextTertiary()).CGColor;
  dot.hidden = [pageID isEqualToString:OCGGeneralPage];
  dot.toolTip = enabled ? OCGT(@"Shown in the menu bar popover") : OCGT(@"Switched off");
  dot.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:dot];

  [NSLayoutConstraint activateConstraints:@[
    [iconView.leadingAnchor constraintEqualToAnchor:row.leadingAnchor constant:10],
    [iconView.centerYAnchor constraintEqualToAnchor:row.centerYAnchor],
    [iconView.widthAnchor constraintEqualToConstant:16],
    [iconView.heightAnchor constraintEqualToConstant:16],
    [label.leadingAnchor constraintEqualToAnchor:iconView.trailingAnchor constant:9],
    [label.centerYAnchor constraintEqualToAnchor:row.centerYAnchor],
    [label.trailingAnchor constraintLessThanOrEqualToAnchor:dot.leadingAnchor constant:-6],
    [dot.trailingAnchor constraintEqualToAnchor:row.trailingAnchor constant:-10],
    [dot.centerYAnchor constraintEqualToAnchor:row.centerYAnchor],
    [dot.widthAnchor constraintEqualToConstant:6],
    [dot.heightAnchor constraintEqualToConstant:6],
  ]];
  return row;
}

- (void)renderSidebar {
  for (NSView *v in [self.sidebar.subviews copy]) {
    [v removeFromSuperview];
  }
  NSMutableArray<NSView *> *rows = [NSMutableArray array];
  [rows addObject:[self sidebarRow:OCGGeneralPage
                             title:OCGT(@"General")
                              icon:[NSImage imageWithSystemSymbolName:@"slider.horizontal.3" accessibilityDescription:nil]
                           enabled:YES]];
  NSTextField *caption = OCGLabel(OCGT(@"PROVIDERS"), kOCGFontBadge, NSFontWeightSemibold, OCGTextTertiary());
  [rows addObject:caption];
  for (NSDictionary *p in self.state[@"providers"] ?: @[]) {
    NSString *pid = p[@"id"] ?: @"";
    [rows addObject:[self sidebarRow:pid
                               title:p[@"label"] ?: pid
                                icon:OCGLogoImageForProvider(pid)
                             enabled:[p[@"enabled"] boolValue]]];
  }
  NSView *previous = nil;
  for (NSView *row in rows) {
    [self.sidebar addSubview:row];
    BOOL isCaption = (row == caption);
    [NSLayoutConstraint activateConstraints:@[
      [row.leadingAnchor constraintEqualToAnchor:self.sidebar.leadingAnchor constant:(isCaption ? 20 : 10)],
      [row.trailingAnchor constraintEqualToAnchor:self.sidebar.trailingAnchor constant:-10],
      [row.heightAnchor constraintEqualToConstant:(isCaption ? 14 : 30)],
      previous == nil ? [row.topAnchor constraintEqualToAnchor:self.sidebar.topAnchor constant:18]
                      : [row.topAnchor constraintEqualToAnchor:previous.bottomAnchor constant:(isCaption ? 18 : 3)],
    ]];
    if (isCaption) {
      caption.translatesAutoresizingMaskIntoConstraints = NO;
    }
    previous = row;
  }
}

- (void)pageClicked:(NSButton *)sender {
  [self.view.window makeFirstResponder:nil]; // commit a field being edited
  self.editingText = NO;
  [self showPage:sender.identifier];
}

// ---------- content ----------

- (void)renderContent {
  for (NSView *v in [self.content.subviews copy]) {
    [v removeFromSuperview];
  }
  NSView *previous = nil;
  if ([self.selectedPage isEqualToString:OCGGeneralPage] || [self providerInfo:self.selectedPage] == nil) {
    [self renderGeneralPage:&previous];
  } else {
    [self renderProviderPage:self.selectedPage previous:&previous];
  }
  if (previous != nil) {
    [previous.bottomAnchor constraintEqualToAnchor:self.content.bottomAnchor constant:-24].active = YES;
  }
}

- (void)addPageHeader:(NSString *)title icon:(NSImage *)icon blurb:(NSString *)blurb previous:(NSView **)previous {
  NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];
  NSImageView *iconView = [[NSImageView alloc] initWithFrame:NSZeroRect];
  iconView.image = icon;
  iconView.contentTintColor = OCGTextPrimary();
  iconView.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:iconView];
  NSTextField *titleLabel = OCGLabel(title, kOCGFontTitle, NSFontWeightBold, OCGTextPrimary());
  [row addSubview:titleLabel];
  NSTextField *blurbLabel = OCGLabel(blurb, kOCGFontDetail, NSFontWeightRegular, OCGTextSecondary());
  blurbLabel.lineBreakMode = NSLineBreakByWordWrapping;
  blurbLabel.maximumNumberOfLines = 2;
  [blurbLabel setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                       forOrientation:NSLayoutConstraintOrientationHorizontal];
  [row addSubview:blurbLabel];
  [NSLayoutConstraint activateConstraints:@[
    [iconView.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
    [iconView.centerYAnchor constraintEqualToAnchor:titleLabel.centerYAnchor],
    [iconView.widthAnchor constraintEqualToConstant:20],
    [iconView.heightAnchor constraintEqualToConstant:20],
    [titleLabel.leadingAnchor constraintEqualToAnchor:iconView.trailingAnchor constant:10],
    [titleLabel.topAnchor constraintEqualToAnchor:row.topAnchor],
    [blurbLabel.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
    [blurbLabel.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
    [blurbLabel.topAnchor constraintEqualToAnchor:titleLabel.bottomAnchor constant:8],
  ]];
  OCGAddSettingsRow(self.content, row, 64, previous, 0);
}

/// A small caps caption over a group of rows.
- (void)addSection:(NSString *)title previous:(NSView **)previous {
  NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];
  NSTextField *label = OCGLabel(title, kOCGFontBadge, NSFontWeightSemibold, OCGTextTertiary());
  [row addSubview:label];
  NSView *rule = [[NSView alloc] initWithFrame:NSZeroRect];
  rule.wantsLayer = YES;
  rule.layer.backgroundColor = OCGBorderColor().CGColor;
  rule.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:rule];
  [NSLayoutConstraint activateConstraints:@[
    [label.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
    [label.bottomAnchor constraintEqualToAnchor:row.bottomAnchor constant:-6],
    [rule.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
    [rule.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
    [rule.bottomAnchor constraintEqualToAnchor:row.bottomAnchor],
    [rule.heightAnchor constraintEqualToConstant:1],
  ]];
  OCGAddSettingsRow(self.content, row, 22, previous, 22);
}

/// "Title ........ [control]" with an optional hint under the title.
- (void)addSettingRow:(NSString *)title hint:(NSString *)hint control:(NSView *)control previous:(NSView **)previous {
  NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];
  NSTextField *label = OCGLabel(title, kOCGFontBody, NSFontWeightRegular, OCGTextPrimary());
  [row addSubview:label];
  control.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:control];
  NSMutableArray *c = [NSMutableArray arrayWithArray:@[
    [label.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
    [control.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
    [control.centerYAnchor constraintEqualToAnchor:row.centerYAnchor],
    [label.trailingAnchor constraintLessThanOrEqualToAnchor:control.leadingAnchor constant:-12],
  ]];
  if (hint.length > 0) {
    NSTextField *hintLabel = OCGLabel(hint, kOCGFontDetail, NSFontWeightRegular, OCGTextTertiary());
    [row addSubview:hintLabel];
    [c addObjectsFromArray:@[
      [label.topAnchor constraintEqualToAnchor:row.topAnchor constant:2],
      [hintLabel.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
      [hintLabel.topAnchor constraintEqualToAnchor:label.bottomAnchor constant:3],
      [hintLabel.trailingAnchor constraintLessThanOrEqualToAnchor:control.leadingAnchor constant:-12],
    ]];
  } else {
    [c addObject:[label.centerYAnchor constraintEqualToAnchor:row.centerYAnchor]];
  }
  [NSLayoutConstraint activateConstraints:c];
  OCGAddSettingsRow(self.content, row, hint.length > 0 ? 38 : 26, previous, 12);
}

- (OCGToggle *)toggleOn:(BOOL)on identifier:(NSString *)identifier action:(SEL)action {
  OCGToggle *t = [[OCGToggle alloc] init];
  t.isOn = on;
  t.identifier = identifier;
  t.target = self;
  t.action = action;
  return t;
}

// ---------- General ----------

- (void)renderGeneralPage:(NSView **)previous {
  [self addPageHeader:OCGT(@"General")
                 icon:[NSImage imageWithSystemSymbolName:@"slider.horizontal.3" accessibilityDescription:nil]
                blurb:OCGT(@"How every provider's quotas are read, and how often they are fetched.")
             previous:previous];

  [self addSection:OCGT(@"LANGUAGE") previous:previous];
  // Each language names itself, so whoever cannot read the current one can
  // still find their own.
  OCGModePicker *language = [[OCGModePicker alloc] initWithLabels:@[ OCGT(@"SYSTEM"), @"ENGLISH", @"简体中文" ]];
  language.target = self;
  language.action = @selector(languageChanged:);
  NSString *setting = self.state[@"language_setting"];
  language.selectedSegment = [setting isEqual:@"en"] ? 1 : [setting isEqual:@"zh-Hans"] ? 2 : 0;
  [self addSettingRow:OCGT(@"Language")
                 hint:OCGT(@"Menus, cards and messages. System follows macOS.")
              control:language
             previous:previous];

  [self addSection:OCGT(@"METERS") previous:previous];
  OCGModePicker *mode = [[OCGModePicker alloc] initWithLabels:@[ OCGT(@"USED"), OCGT(@"REMAINING") ]];
  mode.target = self;
  mode.action = @selector(meterModeChanged:);
  mode.selectedSegment = [self.state[@"codex_show_remaining"] boolValue] ? 1 : 0;
  [self addSettingRow:OCGT(@"Read quotas as")
                 hint:OCGT(@"The menu bar number and every bar in the popover follow this.")
              control:mode
             previous:previous];

  [self addSection:OCGT(@"REFRESH") previous:previous];
  NSView *refresh = [[NSView alloc] initWithFrame:NSZeroRect];
  NSTextField *minutes = [[NSTextField alloc] initWithFrame:NSZeroRect];
  NSNumber *current = self.state[@"refresh_minutes"];
  minutes.stringValue = [current isKindOfClass:[NSNumber class]] ? current.stringValue : @"15";
  minutes.identifier = @"refresh_minutes";
  minutes.delegate = self;
  minutes.alignment = NSTextAlignmentRight;
  minutes.controlSize = NSControlSizeSmall;
  minutes.font = OCGMonoFont(kOCGFontBody, NSFontWeightRegular);
  minutes.toolTip = OCGT(@"Minutes between background refreshes (1–240)");
  NSNumberFormatter *digits = [[NSNumberFormatter alloc] init];
  digits.numberStyle = NSNumberFormatterNoStyle;
  digits.minimum = @1;
  digits.maximum = @240;
  minutes.formatter = digits;
  minutes.translatesAutoresizingMaskIntoConstraints = NO;
  [refresh addSubview:minutes];
  NSTextField *unit = OCGLabel(OCGT(@"min"), kOCGFontBody, NSFontWeightRegular, OCGTextSecondary());
  [refresh addSubview:unit];
  [NSLayoutConstraint activateConstraints:@[
    [minutes.leadingAnchor constraintEqualToAnchor:refresh.leadingAnchor],
    [minutes.centerYAnchor constraintEqualToAnchor:refresh.centerYAnchor],
    [minutes.widthAnchor constraintEqualToConstant:52],
    [unit.leadingAnchor constraintEqualToAnchor:minutes.trailingAnchor constant:6],
    [unit.centerYAnchor constraintEqualToAnchor:refresh.centerYAnchor],
    [unit.trailingAnchor constraintEqualToAnchor:refresh.trailingAnchor],
    [refresh.heightAnchor constraintEqualToConstant:24],
  ]];
  [self addSettingRow:OCGT(@"Refresh every")
                 hint:OCGT(@"The refresh button in the popover fetches right away.")
              control:refresh
             previous:previous];
}

- (void)languageChanged:(OCGModePicker *)sender {
  NSArray *settings = @[ @"", @"en", @"zh-Hans" ];
  NSInteger i = sender.selectedSegment;
  goSaveLanguage([settings[(i >= 0 && i < 3) ? i : 0] UTF8String]);
}

- (void)meterModeChanged:(OCGModePicker *)sender {
  [self saveDisplaySettingsWith:@{@"show_remaining" : @(sender.selectedSegment == 1)}];
}

/// The display switches travel together; send them all, with `changes`.
- (void)saveDisplaySettingsWith:(NSDictionary *)changes {
  NSMutableDictionary *settings = [@{
    @"show_spend" : @([self.state[@"codex_show_spend"] boolValue]),
    @"show_remaining" : @([self.state[@"codex_show_remaining"] boolValue]),
    @"show_today" : @([self.state[@"codex_show_today"] boolValue]),
    @"show_reset_credits" : @([self.state[@"codex_show_reset_credits"] boolValue]),
  } mutableCopy];
  [settings addEntriesFromDictionary:changes];
  NSData *data = [NSJSONSerialization dataWithJSONObject:settings options:0 error:nil];
  if (data != nil) {
    goSaveCodexAccounts([[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding].UTF8String);
  }
}

// ---------- a provider ----------

- (void)renderProviderPage:(NSString *)provider previous:(NSView **)previous {
  NSDictionary *info = [self providerInfo:provider];
  [self addPageHeader:info[@"label"] ?: provider
                 icon:OCGLogoImageForProvider(provider)
                blurb:OCGProviderBlurb(provider)
             previous:previous];

  OCGToggle *enabled = [self toggleOn:[info[@"enabled"] boolValue]
                           identifier:provider
                               action:@selector(providerToggled:)];
  [self addSettingRow:OCGT(@"Show in the popover")
                 hint:OCGT(@"Off hides the tab and stops fetching it.")
              control:enabled
             previous:previous];

  BOOL canSignIn = [(self.state[@"signin_providers"] ?: @[]) containsObject:provider];
  NSArray *accounts = self.state[@"accounts"][provider];
  if (canSignIn || accounts.count > 0) {
    [self addSection:OCGT(@"ACCOUNTS") previous:previous];
    if (accounts.count == 0) {
      NSTextField *none = OCGLabel(OCGT(@"No account yet."), kOCGFontDetail, NSFontWeightRegular, OCGTextSecondary());
      OCGAddSettingsRow(self.content, none, 16, previous, 12);
    }
    for (NSDictionary *account in accounts) {
      [self addAccountRow:account provider:provider previous:previous];
    }
    if (canSignIn) {
      [self addSignInRow:provider previous:previous];
    }
  }

  NSArray *fields = OCGFieldsForProvider(provider);
  if (fields.count > 0) {
    [self addSection:([provider isEqualToString:@"opencode"] ? OCGT(@"API KEY (OPTIONAL)") : OCGT(@"API KEY")) previous:previous];
    NSDictionary *creds = self.state[@"credentials"][provider] ?: @{};
    for (NSDictionary *def in fields) {
      NSString *field = def[@"field"];
      NSTextField *input = [def[@"secure"] boolValue] ? [[NSSecureTextField alloc] initWithFrame:NSZeroRect]
                                                        : [[NSTextField alloc] initWithFrame:NSZeroRect];
      input.stringValue = creds[field] ?: @"";
      input.placeholderString = def[@"label"];
      input.delegate = self;
      input.identifier = [NSString stringWithFormat:@"cred:%@|%@", provider, field];
      input.controlSize = NSControlSizeSmall;
      input.font = OCGMonoFont(kOCGFontBody, NSFontWeightRegular);
      input.toolTip = OCGT(@"Saved when you leave the field");
      OCGAddSettingsRow(self.content, input, 24, previous, 12);
    }
  }

  if ([provider isEqualToString:@"codex"]) {
    [self addSection:OCGT(@"CARD ROWS") previous:previous];
    [self addSettingRow:OCGT(@"Spend limit")
                   hint:OCGT(@"The workspace spend-control meter, when it is in use.")
                control:[self toggleOn:[self.state[@"codex_show_spend"] boolValue] identifier:@"show_spend" action:@selector(codexRowToggled:)]
               previous:previous];
    [self addSettingRow:OCGT(@"Today's usage")
                   hint:OCGT(@"How much of the 5h window went since midnight.")
                control:[self toggleOn:[self.state[@"codex_show_today"] boolValue] identifier:@"show_today" action:@selector(codexRowToggled:)]
               previous:previous];
    [self addSettingRow:OCGT(@"Reset credits")
                   hint:OCGT(@"How many free window resets the account holds.")
                control:[self toggleOn:[self.state[@"codex_show_reset_credits"] boolValue] identifier:@"show_reset_credits" action:@selector(codexRowToggled:)]
               previous:previous];
  }
}

- (void)codexRowToggled:(OCGToggle *)sender {
  [self saveDisplaySettingsWith:@{sender.identifier : @(sender.isOn)}];
}

- (void)providerToggled:(OCGToggle *)sender {
  NSMutableDictionary *payload = [NSMutableDictionary dictionary];
  for (NSDictionary *p in self.state[@"providers"] ?: @[]) {
    payload[p[@"id"]] = @([p[@"id"] isEqualToString:sender.identifier] ? sender.isOn : [p[@"enabled"] boolValue]);
  }
  NSData *data = [NSJSONSerialization dataWithJSONObject:payload options:0 error:nil];
  if (data != nil) {
    goSaveProviderEnabled([[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding].UTF8String);
  }
}

// ---------- accounts ----------

- (void)addAccountRow:(NSDictionary *)account provider:(NSString *)provider previous:(NSView **)previous {
  NSString *key = account[@"key"] ?: @"";
  NSString *source = account[@"source"] ?: @"";
  NSString *problem = account[@"error"];
  BOOL hasProblem = [problem isKindOfClass:[NSString class]] && problem.length > 0;
  BOOL removable = [source isEqualToString:@"saved"];
  BOOL switchable = [provider isEqualToString:@"codex"] && removable && !hasProblem;

  NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];
  row.wantsLayer = YES;
  row.layer.cornerRadius = 8;
  row.layer.backgroundColor = OCGCardColor().CGColor;
  row.layer.borderWidth = 1;
  row.layer.borderColor = OCGBorderColor().CGColor;

  NSTextField *name = [[NSTextField alloc] initWithFrame:NSZeroRect];
  name.stringValue = account[@"label"] ?: @"";
  NSString *fallback = [account[@"name"] length] > 0 ? account[@"name"] : @"name";
  name.placeholderString = fallback;
  name.toolTip = OCGT(@"Click to rename — shown on the card instead of the email");
  name.font = OCGMonoFont(kOCGFontBody, NSFontWeightMedium);
  name.textColor = OCGTextPrimary();
  name.bordered = NO;
  name.bezeled = NO;
  name.drawsBackground = NO;
  name.focusRingType = NSFocusRingTypeNone;
  name.delegate = self;
  name.identifier = [NSString stringWithFormat:@"label:%@|%@", provider, key];
  name.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:name];

  NSMutableArray *parts = [NSMutableArray arrayWithObject:OCGSourceLabel(provider, account)];
  if ([account[@"plan"] length] > 0) {
    [parts addObject:account[@"plan"]];
  }
  if ([account[@"label"] length] > 0 && [account[@"name"] length] > 0) {
    [parts addObject:account[@"name"]];
  }
  NSString *subtitle = hasProblem ? problem : [parts componentsJoinedByString:@" · "];
  NSColor *subtitleColor = hasProblem ? OCGAccentAmber()
                                      : ([account[@"active"] boolValue] ? OCGAccentGreen() : OCGTextTertiary());
  NSTextField *sub = OCGLabel(subtitle, kOCGFontDetail, NSFontWeightRegular, subtitleColor);
  sub.toolTip = [source isEqualToString:@"cli"]
                    ? OCGT(@"The CLI's own login — tokue reads it but never changes it; sign out with the CLI.")
                    : subtitle;
  [row addSubview:sub];

  OCGToggle *shown = [self toggleOn:[account[@"enabled"] boolValue]
                         identifier:[NSString stringWithFormat:@"%@|%@", provider, key]
                             action:@selector(accountShownToggled:)];
  shown.toolTip = OCGT(@"Show this account in the popover");
  shown.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:shown];

  NSLayoutXAxisAnchor *textLimit = shown.leadingAnchor;
  NSMutableArray *c = [NSMutableArray array];
  if (removable) {
    NSButton *remove = OCGLinkButton(OCGT(@"REMOVE"), OCGTextTertiary(), self, @selector(removeAccountClicked:));
    remove.identifier = [NSString stringWithFormat:@"%@|%@", provider, key];
    remove.toolTip = OCGT(@"Forget this account's login");
    [row addSubview:remove];
    [c addObjectsFromArray:@[
      [remove.trailingAnchor constraintEqualToAnchor:shown.leadingAnchor constant:-14],
      [remove.centerYAnchor constraintEqualToAnchor:row.centerYAnchor],
    ]];
    textLimit = remove.leadingAnchor;
    if (switchable) {
      NSButton *use = OCGLinkButton(OCGT(@"USE IN CODEX"), OCGAccentGreen(), self, @selector(useAccountClicked:));
      use.identifier = key;
      use.toolTip = OCGT(@"Sign Codex in to this account; its current one is kept here instead");
      [row addSubview:use];
      [c addObjectsFromArray:@[
        [use.trailingAnchor constraintEqualToAnchor:remove.leadingAnchor constant:-12],
        [use.centerYAnchor constraintEqualToAnchor:row.centerYAnchor],
      ]];
      textLimit = use.leadingAnchor;
    }
  }
  [c addObjectsFromArray:@[
    [shown.trailingAnchor constraintEqualToAnchor:row.trailingAnchor constant:-12],
    [shown.centerYAnchor constraintEqualToAnchor:row.centerYAnchor],
    [name.leadingAnchor constraintEqualToAnchor:row.leadingAnchor constant:12],
    [name.trailingAnchor constraintLessThanOrEqualToAnchor:textLimit constant:-10],
    [name.widthAnchor constraintGreaterThanOrEqualToConstant:120],
    [name.topAnchor constraintEqualToAnchor:row.topAnchor constant:8],
    [sub.leadingAnchor constraintEqualToAnchor:name.leadingAnchor],
    [sub.trailingAnchor constraintLessThanOrEqualToAnchor:textLimit constant:-10],
    [sub.topAnchor constraintEqualToAnchor:name.bottomAnchor constant:2],
  ]];
  [NSLayoutConstraint activateConstraints:c];
  OCGAddSettingsRow(self.content, row, 50, previous, 8);
}

/// "Add account", or where a sign-in stands: a device code to type, or a
/// wait for the browser — with a way out of either.
- (void)addSignInRow:(NSString *)provider previous:(NSView **)previous {
  NSDictionary *signin = self.state[@"signins"][provider];
  NSString *state = [signin isKindOfClass:[NSDictionary class]] ? signin[@"state"] : @"";
  NSString *message = [signin isKindOfClass:[NSDictionary class]] ? signin[@"message"] : @"";
  NSString *code = [signin isKindOfClass:[NSDictionary class]] ? signin[@"code"] : nil;
  BOOL waiting = [state isEqualToString:@"waiting"];

  if (waiting && [code isKindOfClass:[NSString class]] && code.length > 0) {
    NSView *box = [[NSView alloc] initWithFrame:NSZeroRect];
    box.wantsLayer = YES;
    box.layer.cornerRadius = 8;
    box.layer.borderWidth = 1;
    box.layer.borderColor = OCGSelectedBorderColor().CGColor;
    NSTextField *hint = OCGLabel(OCGT(@"Confirm this code in the browser"), kOCGFontDetail, NSFontWeightRegular, OCGTextSecondary());
    [box addSubview:hint];
    NSTextField *codeLabel = OCGLabel(code, 22, NSFontWeightBold, OCGTextPrimary());
    codeLabel.selectable = YES;
    [box addSubview:codeLabel];
    NSButton *copy = OCGLinkButton(OCGT(@"COPY"), OCGAccentGreen(), self, @selector(copyCodeClicked:));
    copy.identifier = code;
    [box addSubview:copy];
    NSButton *openPage = OCGLinkButton(OCGT(@"OPEN PAGE"), OCGAccentGreen(), self, @selector(openSignInPageClicked:));
    openPage.identifier = signin[@"url"] ?: @"";
    [box addSubview:openPage];
    NSButton *cancel = OCGLinkButton(OCGT(@"CANCEL"), OCGTextTertiary(), self, @selector(cancelSignInClicked:));
    cancel.identifier = provider;
    [box addSubview:cancel];
    [NSLayoutConstraint activateConstraints:@[
      [hint.leadingAnchor constraintEqualToAnchor:box.leadingAnchor constant:14],
      [hint.topAnchor constraintEqualToAnchor:box.topAnchor constant:12],
      [codeLabel.leadingAnchor constraintEqualToAnchor:hint.leadingAnchor],
      [codeLabel.topAnchor constraintEqualToAnchor:hint.bottomAnchor constant:6],
      [cancel.trailingAnchor constraintEqualToAnchor:box.trailingAnchor constant:-14],
      [cancel.centerYAnchor constraintEqualToAnchor:codeLabel.centerYAnchor],
      [openPage.trailingAnchor constraintEqualToAnchor:cancel.leadingAnchor constant:-14],
      [openPage.centerYAnchor constraintEqualToAnchor:codeLabel.centerYAnchor],
      [copy.trailingAnchor constraintEqualToAnchor:openPage.leadingAnchor constant:-14],
      [copy.centerYAnchor constraintEqualToAnchor:codeLabel.centerYAnchor],
    ]];
    OCGAddSettingsRow(self.content, box, 76, previous, 12);
    return;
  }

  NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];
  NSString *title = waiting ? OCGT(@"CANCEL") : OCGT(@"ADD ACCOUNT");
  NSButton *button = [NSButton buttonWithTitle:title
                                        target:self
                                        action:(waiting ? @selector(cancelSignInClicked:) : @selector(addAccountClicked:))];
  button.identifier = provider;
  if (waiting) {
    OCGStyleSecondaryButton(button, title);
  } else {
    OCGStylePrimaryButton(button, title);
  }
  button.toolTip = OCGT(@"Sign in to another account in the browser");
  button.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:button];
  NSColor *tint = [state isEqualToString:@"failed"] ? OCGAccentAmber() : OCGTextSecondary();
  NSTextField *status = OCGLabel(message ?: @"", kOCGFontDetail, NSFontWeightRegular, tint);
  status.toolTip = message;
  [row addSubview:status];
  [NSLayoutConstraint activateConstraints:@[
    [button.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
    [button.centerYAnchor constraintEqualToAnchor:row.centerYAnchor],
    [button.widthAnchor constraintEqualToConstant:128],
    [button.heightAnchor constraintEqualToConstant:26],
    [status.leadingAnchor constraintEqualToAnchor:button.trailingAnchor constant:12],
    [status.trailingAnchor constraintLessThanOrEqualToAnchor:row.trailingAnchor],
    [status.centerYAnchor constraintEqualToAnchor:row.centerYAnchor],
  ]];
  OCGAddSettingsRow(self.content, row, 28, previous, 14);
}

- (NSDictionary *)account:(NSString *)key of:(NSString *)provider {
  for (NSDictionary *a in self.state[@"accounts"][provider] ?: @[]) {
    if ([a[@"key"] isEqualToString:key]) {
      return a;
    }
  }
  return nil;
}

- (NSString *)nameOf:(NSDictionary *)account {
  if ([account[@"label"] length] > 0) return account[@"label"];
  if ([account[@"name"] length] > 0) return account[@"name"];
  return OCGT(@"this account");
}

- (void)accountShownToggled:(OCGToggle *)sender {
  NSArray *parts = [sender.identifier componentsSeparatedByString:@"|"];
  if (parts.count < 2) return;
  NSString *provider = parts[0];
  NSString *key = [[parts subarrayWithRange:NSMakeRange(1, parts.count - 1)] componentsJoinedByString:@"|"];
  NSDictionary *a = [self account:key of:provider];
  goSetAccount(provider.UTF8String, key.UTF8String, [a[@"label"] ?: @"" UTF8String], sender.isOn);
}

- (void)addAccountClicked:(NSButton *)sender {
  goSignIn(sender.identifier.UTF8String);
}

- (void)cancelSignInClicked:(NSButton *)sender {
  goCancelSignIn(sender.identifier.UTF8String);
}

- (void)copyCodeClicked:(NSButton *)sender {
  [[NSPasteboard generalPasteboard] clearContents];
  [[NSPasteboard generalPasteboard] setString:sender.identifier forType:NSPasteboardTypeString];
}

- (void)openSignInPageClicked:(NSButton *)sender {
  NSURL *url = [NSURL URLWithString:sender.identifier ?: @""];
  if (url != nil) {
    [[NSWorkspace sharedWorkspace] openURL:url];
  }
}

/// Switching rewrites ~/.codex/auth.json — the login the Codex CLI itself
/// uses — so it is confirmed first.
- (void)useAccountClicked:(NSButton *)sender {
  NSString *key = sender.identifier ?: @"";
  NSString *name = [self nameOf:[self account:key of:@"codex"]];
  NSString *current = nil;
  for (NSDictionary *a in self.state[@"accounts"][@"codex"] ?: @[]) {
    if ([a[@"active"] boolValue]) current = [self nameOf:a];
  }
  NSAlert *alert = [[NSAlert alloc] init];
  alert.messageText = [NSString stringWithFormat:OCGT(@"Use %@ in Codex?"), name];
  alert.informativeText = [NSString
      stringWithFormat:OCGT(@"Codex will be signed in to %@.%@ Codex sessions already running keep their account until they are restarted."),
                       name,
                       current ? [NSString stringWithFormat:OCGT(@" %@ is kept here as a saved account."), current] : @""];
  [alert addButtonWithTitle:OCGT(@"Use in Codex")];
  [alert addButtonWithTitle:OCGT(@"Cancel")];
  [alert beginSheetModalForWindow:self.view.window
                completionHandler:^(NSModalResponse response) {
                  if (response == NSAlertFirstButtonReturn) {
                    goCodexUseAccount(key.UTF8String);
                  }
                }];
}

- (void)removeAccountClicked:(NSButton *)sender {
  NSArray *parts = [sender.identifier componentsSeparatedByString:@"|"];
  if (parts.count < 2) return;
  NSString *provider = parts[0];
  NSString *key = [[parts subarrayWithRange:NSMakeRange(1, parts.count - 1)] componentsJoinedByString:@"|"];
  NSString *name = [self nameOf:[self account:key of:provider]];
  NSAlert *alert = [[NSAlert alloc] init];
  alert.messageText = [NSString stringWithFormat:OCGT(@"Remove %@?"), name];
  alert.informativeText = OCGT(@"Its login is deleted from tokue. To track it again, add it with Add Account.");
  alert.alertStyle = NSAlertStyleWarning;
  [alert addButtonWithTitle:OCGT(@"Remove")];
  [alert addButtonWithTitle:OCGT(@"Cancel")];
  [alert beginSheetModalForWindow:self.view.window
                completionHandler:^(NSModalResponse response) {
                  if (response == NSAlertFirstButtonReturn) {
                    goRemoveAccount(provider.UTF8String, key.UTF8String);
                  }
                }];
}

// ---------- NSTextFieldDelegate ----------

- (void)controlTextDidBeginEditing:(NSNotification *)note {
  self.editingText = YES;
}

- (void)controlTextDidEndEditing:(NSNotification *)note {
  self.editingText = NO;
  NSTextField *field = note.object;
  NSString *ident = field.identifier ?: @"";
  if ([ident hasPrefix:@"cred:"]) {
    NSArray *parts = [[ident substringFromIndex:5] componentsSeparatedByString:@"|"];
    if (parts.count == 2) {
      goSaveCredentials([parts[0] UTF8String], [parts[1] UTF8String], field.stringValue.UTF8String);
    }
  } else if ([ident hasPrefix:@"label:"]) {
    NSString *rest = [ident substringFromIndex:6];
    NSRange bar = [rest rangeOfString:@"|"];
    if (bar.location == NSNotFound) return;
    NSString *provider = [rest substringToIndex:bar.location];
    NSString *key = [rest substringFromIndex:bar.location + 1];
    NSDictionary *a = [self account:key of:provider];
    if ([(a[@"label"] ?: @"") isEqualToString:field.stringValue]) return;
    goSetAccount(provider.UTF8String, key.UTF8String, field.stringValue.UTF8String, [a[@"enabled"] boolValue]);
  } else if ([ident isEqualToString:@"refresh_minutes"]) {
    NSInteger value = field.integerValue;
    if (value < 1 || value > 240) {
      NSNumber *current = self.state[@"refresh_minutes"];
      field.stringValue = [current isKindOfClass:[NSNumber class]] ? current.stringValue : @"15";
      return;
    }
    goSaveRefreshMinutes((unsigned int)value);
  }
}

@end

// ---------------------------------------------------------------------------
// AppDelegate: owns the NSStatusItem and the NSPopover.
// ---------------------------------------------------------------------------

@interface OCGAppDelegate : NSObject <NSApplicationDelegate, OCGPreferencesOpening>
@property(strong) NSStatusItem *statusItem;
@property(strong) NSPopover *popover;
@property(strong) UsagePanelController *controller;
@property(strong) NSWindow *preferencesWindow;
@property(strong) SettingsWindowController *settingsController;
@end

@implementation OCGAppDelegate

- (void)applicationDidFinishLaunching:(NSNotification *)note {
  [[NSProcessInfo processInfo] disableAutomaticTermination:@"tokue menu bar monitor"];

  self.statusItem =
      [[NSStatusBar systemStatusBar] statusItemWithLength:NSVariableStatusItemLength];
  self.statusItem.button.target = self;
  self.statusItem.button.action = @selector(togglePopover:);
  // No icon until the first refresh pushes a template gauge icon.

  self.controller = [[UsagePanelController alloc] init];
  self.controller.appDelegate = self;
  self.popover = [[NSPopover alloc] init];
  self.popover.contentViewController = self.controller;
  self.popover.behavior = NSPopoverBehaviorTransient;
  self.popover.animates = YES;
  // The popover is a fixed dark surface regardless of system appearance —
  // force dark so AppKit's own popover chrome (border, arrow) matches it.
  self.popover.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua];

  // "Follow the system" needs to know the system's language.
  goSetSystemLanguage([NSLocale preferredLanguages].firstObject.UTF8String ?: "");
  // Hand off to the Rust data layer — starts the background refresh loop.
  goOnReady();







  // Debug hook: `TOKUE_AUTOOPEN=1` shows the popover shortly after launch so the
  // panel can be inspected without hunting for the status item.
  if (getenv("TOKUE_AUTOOPEN") != NULL) {
    // Keep it on screen even while other apps take focus, so a screenshot can
    // capture the panel.
    self.popover.behavior = NSPopoverBehaviorApplicationDefined;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(6.0 * NSEC_PER_SEC)),
                   dispatch_get_main_queue(), ^{
                     [self togglePopover:nil];
                   });
  }
}

- (void)togglePopover:(id)sender {
  if (self.popover.shown) {
    [self.popover performClose:nil];
    return;
  }
  [self.popover showRelativeToRect:self.statusItem.button.bounds
                            ofView:self.statusItem.button
                     preferredEdge:NSMinYEdge];
  [NSApp activateIgnoringOtherApps:YES];
}

/// Settings lives in its own window now, separate from the transient
/// popover — open (or front) it, lazily creating it on first use.
- (void)showPreferencesWindow {
  if (self.popover.shown) {
    [self.popover performClose:nil];
  }
  if (self.preferencesWindow == nil) {
    self.settingsController = [[SettingsWindowController alloc] init];
    NSUInteger style = NSWindowStyleMaskTitled | NSWindowStyleMaskClosable |
                        NSWindowStyleMaskMiniaturizable;
    self.preferencesWindow = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 720, 540)
                                                          styleMask:style
                                                            backing:NSBackingStoreBuffered
                                                              defer:NO];
    self.preferencesWindow.title = OCGT(@"Preferences");
    self.preferencesWindow.titlebarAppearsTransparent = YES;
    self.preferencesWindow.backgroundColor = OCGPanelColor();
    // The window is always dark, regardless of the system appearance, so the
    // native title bar (traffic lights, title text) renders correctly against it.
    self.preferencesWindow.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua];
    self.preferencesWindow.contentViewController = self.settingsController;
    self.preferencesWindow.releasedWhenClosed = NO;
    [self.preferencesWindow center];
  }
  // Open on the provider being looked at: the gear next to its name is the
  // way to its settings.
  [self.settingsController showPage:self.controller.state[@"active"]];
  [self.settingsController updateWithState:self.controller.state];
  [NSApp activateIgnoringOtherApps:YES];
  [self.preferencesWindow makeKeyAndOrderFront:nil];
}

/// The menu bar item: a 20pt ring with the badge number inside it. The arc
/// shows the same quantity as the number (quota left, or used, matching the
/// panel). At 100 the ring is simply full and carries no number: three digits
/// do not fit a 9pt face inside a 16pt hole, and a full ring already says it.
///
/// Colour follows the battery icon's rule — quiet by default, colour only for
/// an exception. While quota is healthy the whole thing is a template image
/// that the system tints to the menu bar (white on dark, black on light), so
/// nothing competes for attention. Under 30% left it turns orange and under
/// 10% red, in the vivid system colours on either appearance, which is what
/// makes those states land. With no data it is a dim template ring.
static void OCGDrawGauge(NSRect rect, int pct, int severity, BOOL hasData);
static BOOL OCGGaugeIsAlarmed(int severity, BOOL hasData);

- (void)setStatusGaugePercent:(int)percent severity:(int)severity hasData:(BOOL)hasData {
  // Debug aid: TOKUE_STATUS_BADGE=42 forces the number so the item is easy to
  // find in screenshots.
  const char *forced = getenv("TOKUE_STATUS_BADGE");
  if (forced != NULL) {
    percent = atoi(forced);
    hasData = YES;
  }
  const int pct = MAX(0, MIN(100, percent));
  NSImage *image = [NSImage imageWithSize:NSMakeSize(20, 20)
                                  flipped:NO
                           drawingHandler:^BOOL(NSRect rect) {
    OCGDrawGauge(rect, pct, severity, hasData);
    return YES;
  }];
  image.template = !OCGGaugeIsAlarmed(severity, hasData);
  dispatch_async(dispatch_get_main_queue(), ^{
    NSStatusBarButton *button = self.statusItem.button;
    button.image = image;
    button.imagePosition = NSImageOnly;
    button.attributedTitle = [[NSAttributedString alloc] initWithString:@""];
  });
}

/// Draws the gauge into `rect` (20x20 expected) for the current appearance.
/// Under 30% left (severity is always the used side) the gauge stops being
/// quiet.
static BOOL OCGGaugeIsAlarmed(int severity, BOOL hasData) {
  return hasData && severity > 70;
}

static void OCGDrawGauge(NSRect rect, int pct, int severity, BOOL hasData) {
  {
    // In template mode only alpha is read; the system supplies the colour, so
    // black is just "ink". Alarmed, the image is drawn as-is in a system
    // colour — orange rather than yellow, because a 1.75pt yellow ring is
    // unreadable on a light bar.
    BOOL alarmed = OCGGaugeIsAlarmed(severity, hasData);
    NSColor *ink = !alarmed ? NSColor.blackColor
                   : (severity > 90 ? NSColor.systemRedColor : NSColor.systemOrangeColor);
    BOOL dark = [[NSAppearance currentDrawingAppearance].name.lowercaseString containsString:@"dark"];
    CGFloat trackAlpha = !hasData ? 0.42 : (alarmed ? (dark ? 0.30 : 0.24) : 0.28);
    NSColor *tint = ink;
    NSPoint centre = NSMakePoint(NSMidX(rect), NSMidY(rect));
    // 1.4pt puts the ring's ink on a par with the system's own menu bar
    // glyphs (measured: wifi ~26%, clock ~22%, speaker ~30% coverage; a
    // near-full ring at 1.75pt with bold digits was ~35% and read brighter
    // than everything around it).
    const CGFloat thickness = 1.4;
    const CGFloat radius = 9.5 - thickness / 2; // 0.5pt inset keeps the stroke un-clipped

    NSBezierPath *track = [NSBezierPath bezierPath];
    track.lineWidth = thickness;
    [track appendBezierPathWithArcWithCenter:centre radius:radius startAngle:0 endAngle:360];
    [[tint colorWithAlphaComponent:trackAlpha] setStroke];
    [track stroke];

    if (hasData && pct > 0) {
      NSBezierPath *arc = [NSBezierPath bezierPath];
      arc.lineWidth = thickness;
      arc.lineCapStyle = NSLineCapStyleButt;
      [arc appendBezierPathWithArcWithCenter:centre
                                      radius:radius
                                  startAngle:90
                                    endAngle:90 - 360.0 * pct / 100.0
                                   clockwise:YES];
      // Quiet state: the arc is the "fill" and sits a touch under full, the
      // way the battery glyph's body does, while the number stays full as the
      // label. A near-closed ring at full strength reads as a solid disc next
      // to the open shapes around it. Alarmed, everything is full — the
      // colour is meant to be loud.
      [(alarmed ? tint : [tint colorWithAlphaComponent:0.85]) setStroke];
      [arc stroke];
    }

    if (hasData && pct < 100) {
      NSFont *font = [NSFont monospacedDigitSystemFontOfSize:9 weight:NSFontWeightMedium];
      NSString *text = [NSString stringWithFormat:@"%d", pct];
      NSDictionary *attributes = @{NSFontAttributeName : font, NSForegroundColorAttributeName : tint};
      NSSize size = [text sizeWithAttributes:attributes];
      // Centre the cap-height box, not the line box — digits have no
      // descenders, so the line box would sit them visibly low.
      CGFloat baseline = centre.y - font.capHeight / 2;
      [text drawAtPoint:NSMakePoint(centre.x - size.width / 2, baseline + font.descender)
         withAttributes:attributes];
    }
  }
}

- (void)setTooltip:(NSString *)tooltip {
  dispatch_async(dispatch_get_main_queue(), ^{
    self.statusItem.button.toolTip = tooltip;
  });
}

- (void)updateWithStateJSON:(NSString *)json {
  dispatch_async(dispatch_get_main_queue(), ^{
    [self.controller updateWithStateJSON:json];
    [self.settingsController updateWithState:self.controller.state];
    self.preferencesWindow.title = OCGT(@"Preferences");
    [self writeSnapshotIfRequested];
  });
}

/// Renders `view` at `size` off-screen and writes it to `path` as a PNG.
- (void)writeSnapshotOfView:(NSView *)view size:(NSSize)size toPath:(const char *)path {
  [view setFrameSize:size];
  [view layoutSubtreeIfNeeded];
  NSRect bounds = view.bounds;
  NSBitmapImageRep *rep = [view bitmapImageRepForCachingDisplayInRect:bounds];
  [view cacheDisplayInRect:bounds toBitmapImageRep:rep];
  NSData *png = [rep representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
  [png writeToFile:[NSString stringWithUTF8String:path] atomically:YES];
  fprintf(stderr, "[ocg] panel snapshot written to %s (%.0fx%.0f)\n", path, bounds.size.width,
          bounds.size.height);
}

/// Debug hook: `TOKUE_SNAPSHOT=/tmp/panel.png` renders the popover off-screen
/// and writes a PNG, so the layout can be inspected without hunting for the
/// status item. `TOKUE_SNAPSHOT_SETTINGS=<page>` captures that Preferences page
/// instead — `general` or a provider id (`1`: General).
- (void)writeSnapshotIfRequested {
  static BOOL written = NO;
  const char *path = getenv("TOKUE_SNAPSHOT");
  if (written || path == NULL) {
    return;
  }
  written = YES;

  if (getenv("TOKUE_SNAPSHOT_SETTINGS") != NULL) {
    SettingsWindowController *settings = [[SettingsWindowController alloc] init];
    NSWindow *offscreen = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 720, 540)
                                                       styleMask:NSWindowStyleMaskBorderless
                                                         backing:NSBackingStoreBuffered
                                                           defer:NO];
    offscreen.contentViewController = settings;
    [settings.view setFrameSize:NSMakeSize(720, 540)]; // forces the lazy view to load
    [offscreen setContentSize:NSMakeSize(720, 540)]; // resizing the window (not just the view) is what actually propagates through Auto Layout
    [settings showPage:[NSString stringWithUTF8String:getenv("TOKUE_SNAPSHOT_SETTINGS")]];
    [settings updateWithState:self.controller.state];
    [self writeSnapshotOfView:settings.view size:NSMakeSize(720, 540) toPath:path];
    return;
  }

  NSWindow *offscreen = [[NSWindow alloc]
      initWithContentRect:NSMakeRect(0, 0, 272, 546)
                styleMask:NSWindowStyleMaskBorderless
                  backing:NSBackingStoreBuffered
                    defer:NO];
  offscreen.contentViewController = self.controller;
  // viewWillAppear never fires for an off-screen window, so render by hand.
  self.controller.preferredContentSize = NSMakeSize(272, 546);
  [self.controller.view setFrameSize:NSMakeSize(272, 546)];
  [self.controller renderAll];
  [self.controller.view layoutSubtreeIfNeeded];
  // Follow whatever height the content asked for (a tall account list would
  // otherwise be captured clipped).
  NSSize wanted = self.controller.preferredContentSize;
  [offscreen setContentSize:wanted];
  [self writeSnapshotOfView:self.controller.view size:wanted toPath:path];
}

@end

// ---------------------------------------------------------------------------
// C entry points invoked from Rust (all marshal to the main queue).
// ---------------------------------------------------------------------------

static OCGAppDelegate *ocgAppDelegate = nil;

void runApp(void) {
  @autoreleasepool {
    NSApplication *app = [NSApplication sharedApplication];
    ocgAppDelegate = [[OCGAppDelegate alloc] init];
    app.delegate = ocgAppDelegate;
    [app run];
  }
}

void setStatusGauge(int percent, int severity, int hasData) {
  @autoreleasepool {
    [ocgAppDelegate setStatusGaugePercent:percent severity:severity hasData:(hasData != 0)];
  }
}

void setStatusTooltip(const char *tooltip) {
  @autoreleasepool {
    [ocgAppDelegate setTooltip:[NSString stringWithUTF8String:tooltip ?: ""]];
  }
}

void updatePanelState(const char *stateJSON) {
  @autoreleasepool {
    [ocgAppDelegate updateWithStateJSON:[NSString stringWithUTF8String:stateJSON ?: ""]];
  }
}
