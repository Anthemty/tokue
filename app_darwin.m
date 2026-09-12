#import <Cocoa/Cocoa.h>

// C callbacks (implemented in Rust via #[no_mangle]).
extern void goOnReady(void);
extern void goProviderSelected(const char *providerID);
extern void goRefreshRequested(void);
extern void goSaveCredentials(const char *provider, const char *field, const char *value);
extern void goSaveCodexAccounts(const char *accountsJSON);
extern void goRescanCodexAccounts(void);
extern void goQuitRequested(void);

// ---------------------------------------------------------------------------
// OCGFlipView: flipped container so a scrolled document starts at the top.
// ---------------------------------------------------------------------------

@interface OCGFlipView : NSView
@end

@implementation OCGFlipView
- (BOOL)isFlipped {
  return YES;
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

@interface UsagePanelController : NSViewController <NSTextFieldDelegate>
@property(strong) NSView *sidebar;
@property(strong) NSView *content;
@property(strong) NSView *contentContainer;
@property(strong) NSView *headerHolder;
@property(strong) NSView *footerHolder;
@property(strong) NSScrollView *scrollView;
@property(strong) NSLayoutConstraint *scrollHeightConstraint;
/// Content height at the last layout, to detect growth.
@property CGFloat lastContentHeight;
@property(strong) NSMutableDictionary<NSString *, NSButton *> *providerButtons;
@property(strong) NSDictionary *state;
@property BOOL settingsVisible;
/// Set while the settings pane holds edits that are not saved yet.
@property BOOL settingsDirty;
/// Accounts hidden locally, by CODEX_HOME. Unticking a box hides the account
/// right away; Save & Refresh makes it stick for the next launch too.
@property(strong) NSMutableSet<NSString *> *hiddenHomes;
@property(strong) NSMutableDictionary<NSString *, NSTextField *> *fieldInputs;
/// Rows of the Codex account editor: @{@"home", @"checkbox", @"labelField"}.
@property(strong) NSMutableArray<NSDictionary *> *codexAccountRows;
/// Optional Codex meter toggles (default off).
@property(strong) NSButton *showSpendToggle;
@property(strong) NSButton *showTodayToggle;
/// Codex meter reading: segment 0 = used, 1 = remaining (default).
@property(strong) NSSegmentedControl *usedRemainingControl;
- (void)renderAll;
@end

@implementation UsagePanelController

- (instancetype)init {
  self = [super initWithNibName:nil bundle:nil];
  if (self) {
    _providerButtons = [NSMutableDictionary dictionary];
    _fieldInputs = [NSMutableDictionary dictionary];
    _codexAccountRows = [NSMutableArray array];
    _hiddenHomes = [NSMutableSet set];
    _settingsVisible = NO;
  }
  return self;
}

- (void)loadView {
  NSRect frame = NSMakeRect(0, 0, 260, 360);
  self.view = [[NSView alloc] initWithFrame:frame];
  // Single unified background across the whole popover — no sidebar tint.
  self.view.wantsLayer = YES;
  self.view.layer.backgroundColor = [NSColor windowBackgroundColor].CGColor;

  self.sidebar = [[NSView alloc] initWithFrame:NSZeroRect];
  self.sidebar.translatesAutoresizingMaskIntoConstraints = NO;
  [self.view addSubview:self.sidebar];

  // Header and footer stay put; only the usage rows scroll — four ChatGPT
  // accounts plus their meters are taller than the popover should ever be.
  self.headerHolder = [[NSView alloc] initWithFrame:NSZeroRect];
  self.headerHolder.translatesAutoresizingMaskIntoConstraints = NO;
  [self.view addSubview:self.headerHolder];

  self.footerHolder = [[NSView alloc] initWithFrame:NSZeroRect];
  self.footerHolder.translatesAutoresizingMaskIntoConstraints = NO;
  [self.view addSubview:self.footerHolder];

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

  // Hairline vertical divider between sidebar and content.
  NSView *divider = [[NSView alloc] initWithFrame:NSZeroRect];
  divider.translatesAutoresizingMaskIntoConstraints = NO;
  divider.wantsLayer = YES;
  divider.layer.backgroundColor = [NSColor separatorColor].CGColor;
  [self.view addSubview:divider];

  self.scrollHeightConstraint = [self.scrollView.heightAnchor constraintEqualToConstant:220];
  // Fixed width: long account titles truncate (with a tooltip) instead of
  // stretching the popover.
  [self.view.widthAnchor constraintEqualToConstant:260].active = YES;
  [NSLayoutConstraint activateConstraints:@[
    [self.sidebar.leadingAnchor constraintEqualToAnchor:self.view.leadingAnchor],
    [self.sidebar.topAnchor constraintEqualToAnchor:self.view.topAnchor],
    [self.sidebar.bottomAnchor constraintEqualToAnchor:self.view.bottomAnchor],
    [self.sidebar.widthAnchor constraintEqualToConstant:22],
    [divider.leadingAnchor constraintEqualToAnchor:self.sidebar.trailingAnchor],
    [divider.topAnchor constraintEqualToAnchor:self.view.topAnchor],
    [divider.bottomAnchor constraintEqualToAnchor:self.view.bottomAnchor],
    [divider.widthAnchor constraintEqualToConstant:1],

    [self.headerHolder.leadingAnchor constraintEqualToAnchor:divider.trailingAnchor],
    [self.headerHolder.trailingAnchor constraintEqualToAnchor:self.view.trailingAnchor],
    [self.headerHolder.topAnchor constraintEqualToAnchor:self.view.topAnchor],
    [self.headerHolder.heightAnchor constraintEqualToConstant:40],

    [self.scrollView.leadingAnchor constraintEqualToAnchor:divider.trailingAnchor],
    [self.scrollView.trailingAnchor constraintEqualToAnchor:self.view.trailingAnchor],
    [self.scrollView.topAnchor constraintEqualToAnchor:self.headerHolder.bottomAnchor],
    self.scrollHeightConstraint,

    [self.footerHolder.leadingAnchor constraintEqualToAnchor:divider.trailingAnchor],
    [self.footerHolder.trailingAnchor constraintEqualToAnchor:self.view.trailingAnchor],
    [self.footerHolder.topAnchor constraintEqualToAnchor:self.scrollView.bottomAnchor],
    [self.footerHolder.bottomAnchor constraintEqualToAnchor:self.view.bottomAnchor],
    [self.footerHolder.heightAnchor constraintEqualToConstant:30],

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

/// Size the popover to its content, clamped so long lists scroll instead of
/// growing off-screen.
- (void)updatePreferredSize {
  CGFloat contentHeight = self.content.fittingSize.height;
  if (contentHeight <= 0) {
    return;
  }
  CGFloat scrollHeight = MIN(MAX(contentHeight, 60), 470);
  if (fabs(self.scrollHeightConstraint.constant - scrollHeight) > 0.5) {
    self.scrollHeightConstraint.constant = scrollHeight;
    [self.view layoutSubtreeIfNeeded];
  }
  NSSize size = NSMakeSize(260, scrollHeight + 70);
  if (!NSEqualSizes(self.preferredContentSize, size)) {
    self.preferredContentSize = size;
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
  self.state = parsed;
  if (self.viewIfLoaded == nil) {
    return;
  }
  // A background refresh must not rebuild the settings pane while it holds
  // unsaved edits — that silently reverted tick boxes and names before.
  if (self.settingsVisible && self.settingsDirty) {
    return;
  }
  [self renderAll];
}

// ---------- edit tracking ----------

/// Any control the user touches marks the pane dirty.
- (void)settingsEdited:(id)sender {
  self.settingsDirty = YES;
}

- (void)controlTextDidChange:(NSNotification *)note {
  self.settingsDirty = YES;
}

/// Tick box in the Codex account list: hide/show that account in the panel
/// immediately. Nothing is fetched or written here — the panel simply stops
/// rendering that account's rows; Save & Refresh persists the choice.
- (void)accountToggled:(NSButton *)sender {
  NSString *home = sender.identifier ?: @"";
  if (home.length == 0) {
    return;
  }
  self.settingsDirty = YES;
  if (sender.state == NSControlStateValueOn) {
    [self.hiddenHomes removeObject:home];
  } else {
    [self.hiddenHomes addObject:home];
  }
  [self renderAll];
}

- (void)renderAll {
  [self renderSidebar];
  if (self.settingsVisible) {
    [self renderSettings];
  } else {
    [self renderUsage];
  }
}

// ---------- sidebar ----------

- (void)renderSidebar {
  for (NSView *view in [self.sidebar.subviews copy]) {
    [view removeFromSuperview];
  }
  [self.providerButtons removeAllObjects];

  NSArray *providers = self.state[@"providers"] ?: @[];
  NSString *active = self.state[@"active"] ?: @"";
  NSView *previous = nil;
  for (NSDictionary *provider in providers) {
    NSString *providerID = provider[@"id"] ?: @"";
    NSButton *button = [NSButton buttonWithTitle:@""
                                          target:self
                                          action:@selector(providerClicked:)];
    button.image = [self logoImageForProvider:providerID];
    button.imagePosition = NSImageOnly;
    button.bordered = NO;
    BOOL isActive = [providerID isEqualToString:active];
    button.contentTintColor = isActive ? [NSColor systemTealColor] : [NSColor secondaryLabelColor];
    button.identifier = providerID;
    button.translatesAutoresizingMaskIntoConstraints = NO;
    button.wantsLayer = YES;
    button.layer.cornerRadius = 4;
    if (isActive) {
      button.layer.backgroundColor = [[NSColor labelColor] colorWithAlphaComponent:0.08].CGColor;
    }
    [self.sidebar addSubview:button];
    self.providerButtons[providerID] = button;

    NSMutableArray *constraints = [NSMutableArray array];
    [constraints addObject:[button.centerXAnchor constraintEqualToAnchor:self.sidebar.centerXAnchor]];
    // Square highlight box — the marks are square-ish, so a taller box looked
    // stretched. 20pt in a 22pt rail keeps them hugging the edges.
    [constraints addObject:[button.widthAnchor constraintEqualToConstant:20]];
    [constraints addObject:[button.heightAnchor constraintEqualToConstant:20]];
    if (previous == nil) {
      [constraints addObject:[button.topAnchor constraintEqualToAnchor:self.sidebar.topAnchor constant:10]];
    } else {
      [constraints addObject:[button.topAnchor constraintEqualToAnchor:previous.bottomAnchor constant:6]];
    }
    [NSLayoutConstraint activateConstraints:constraints];
    previous = button;
  }
}

// logoImageForProvider returns a template NSImage built from the provider's
// brand logo SVG path (single-path, viewBox 0 0 24 24, sourced from
// @lobehub/icons Mono variants). Template mode lets contentTintColor recolour
// it for active/inactive states.
- (NSImage *)logoImageForProvider:(NSString *)providerID {
  NSString *path = [self logoPathForProvider:providerID];
  if (path == nil) {
    return [NSImage imageWithSystemSymbolName:@"circle" accessibilityDescription:providerID];
  }
  NSString *svg = [NSString stringWithFormat:
      @"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" "
      @"fill=\"black\" fill-rule=\"evenodd\"><path d=\"%@\"/></svg>", path];
  NSData *svgData = [svg dataUsingEncoding:NSUTF8StringEncoding];
  NSImage *image = [[NSImage alloc] initWithData:svgData];
  image.template = YES;
  image.size = NSMakeSize(15, 15);
  return image;
}

// logoPathForProvider returns the SVG path data for each provider's brand logo.
- (NSString *)logoPathForProvider:(NSString *)providerID {
  static NSDictionary *paths = nil;
  static dispatch_once_t once;
  dispatch_once(&once, ^{
    paths = @{
      @"opencode" :
          @"M16 6H8v12h8V6zm4 16H4V2h16v20z",
      @"deepseek" :
          @"M23.748 4.482c-.254-.124-.364.113-.512.234-.051.039-.094.09-.137.136-"
          @".372.397-.806.657-1.373.626-.829-.046-1.537.214-2.163.848-.133-.782-"
          @".575-1.248-1.247-1.548-.352-.156-.708-.311-.955-.65-.172-.241-.219-"
          @".51-.305-.774-.055-.16-.11-.323-.293-.35-.2-.031-.278.136-.356.276-"
          @".313.572-.434 1.202-.422 1.84.027 1.436.633 2.58 1.838 3.393.137.093.172.187.129.323-"
          @".082.28-.18.552-.266.833-.055.179-.137.217-.329.14a5.526 5.526 0 01-1.736-1.18c-"
          @".857-.828-1.631-1.742-2.597-2.458a11.365 11.365 0 00-.689-.471c-.985-.957.13-1.743.388-1.836.27-"
          @".098.093-.432-.779-.428-.872.004-1.67.295-2.687.684a3.055 3.055 0 01-.465.137 9.597 9.597 0 00-2.883-"
          @".102c-1.885.21-3.39 1.102-4.497 2.623C.082 8.606-.231 10.684.152 12.85c.403 2.284 1.569 4.175 3.36 "
          @"5.653 1.858 1.533 3.997 2.284 6.438 2.14 1.482-.085 3.133-.284 4.994-1.86.47.234.962.327 1.78.397.63.059 "
          @"1.236-.03 1.705-.128.735-.156.684-.837.419-.961-2.155-1.004-1.682-.595-2.113-.926 1.096-1.296 "
          @"2.746-2.642 3.392-7.003.05-.347.007-.565 0-.845-.004-.17.035-.237.23-.256a4.173 4.173 0 001.545-"
          @".475c1.396-.763 1.96-2.015 2.093-3.517.02-.23-.004-.467-.247-.588zM11.581 18c-2.089-1.642-3.102-2.183-"
          @"3.52-2.16-.392.024-.321.471-.235.763.09.288.207.486.371.739.114.167.192.416-.113.603-"
          @".673.416-1.842-.14-1.897-.167-1.361-.802-2.5-1.86-3.301-3.307-.774-1.393-1.224-2.887-1.298-4.482-"
          @".02-.386.093-.522.477-.592a4.696 4.696 0 011.529-.039c2.132.312 3.946 1.265 5.468 2.774.868.86 1.525 "
          @"1.887 2.202 2.891.72 1.066 1.494 2.082 2.48 2.914.348.292.625.514.891.677-.802.09-2.14.11-3.054-.614zm1-6.44a"
          @".306.306 0 01.415-.287.302.302 0 01.2.288.306.306 0 01-.31.307.303.303 0 01-.304-.308zm3.11 1.596c-"
          @".2.081-.399.151-.59.16a1.245 1.245 0 01-.798-.254c-.274-.23-.47-.358-.552-.758a1.73 1.73 0 01.016-"
          @".588c.07-.327-.008-.537-.239-.727-.187-.156-.426-.199-.688-.199a.559.559 0 01-.254-.078c-.11-"
          @".054-.2-.19-.114-.358.028-.054.16-.186.192-.21.356-.202.767-.136 1.146.016.352.144.618.408 1.001.782.391.451.462.576.685.914.176.265.336.537.445.848.067.195-.019.354-.25.452z",
      @"minimax" :
          @"M16.278 2c1.156 0 2.093.927 2.093 2.07v12.501a.74.74 0 00.744.709.74.74 0 00.743-.709V9.099a2.06 "
          @"2.06 0 012.071-2.049A2.06 2.06 0 0124 9.1v6.561a.649.649 0 01-.652.645.649.649 0 01-.653-.645V9.1a.762.762 "
          @"0 00-.766-.758.762.762 0 00-.766.758v7.472a2.037 2.037 0 01-2.048 2.026 2.037 2.037 0 01-2.048-2.026v-12.5a.785 "
          @".785 0 00-.788-.753.785.785 0 00-.789.752l-.001 15.904A2.037 2.037 0 0113.441 22a2.037 2.037 0 01-2.048-"
          @"2.026V18.04c0-.356.292-.645.652-.645.36 0 .652.289.652.645v1.934c0 .263.142.506.372.638.23.131.514.131.744 0a.734.734 "
          @"0 00.372-.638V4.07c0-1.143.937-2.07 2.093-2.07zm-5.674 0c1.156 0 2.093.927 2.093 2.07v11.523a.648.648 0 01-.652.645.648.648 "
          @"0 01-.652-.645V4.07a.785.785 0 00-.789-.78.785.785 0 00-.789.78v14.013a2.06 2.06 0 01-2.07 2.048 2.06 2.06 0 01-2.071-2.048V9.1a.762.762 "
          @"0 00-.766-.758.762.762 0 00-.766.758v3.8a2.06 2.06 0 01-2.071 2.049A2.06 2.06 0 010 12.9v-1.378c0-.357.292-.646.652-.646.36 0 .653.29.653.646V12.9c0 "
          @".418.343.757.766.757s.766-.339.766-.757V9.099a2.06 2.06 0 012.07-2.048 2.06 2.06 0 012.071 2.048v8.984c0 .419.343.758.767.758.423 0 .766-.339.766-"
          @".758V4.07c0-1.143.937-2.07 2.093-2.07z",
      @"codex" :
          @"M8.086.457a6.105 6.105 0 013.046-.415c1.333.153 2.521.72 3.564 1.7a.117.117 0 00.107.029c1.408-.346 2.762-.224 4.061.366l.063.03.154.076c1.357.703 2.33 1.77 2.918 3.198.278.679.418 1.388.421 2.126a5.655 5.655 0 01-.18 1.631.167.167 0 00.04.155 5.982 5.982 0 011.578 2.891c.385 1.901-.01 3.615-1.183 5.14l-.182.22a6.063 6.063 0 01-2.934 1.851.162.162 0 00-.108.102c-.255.736-.511 1.364-.987 1.992-1.199 1.582-2.962 2.462-4.948 2.451-1.583-.008-2.986-.587-4.21-1.736a.145.145 0 00-.14-.032c-.518.167-1.04.191-1.604.185a5.924 5.924 0 01-2.595-.622 6.058 6.058 0 01-2.146-1.781c-.203-.269-.404-.522-.551-.821a7.74 7.74 0 01-.495-1.283 6.11 6.11 0 01-.017-3.064.166.166 0 00.008-.074.115.115 0 00-.037-.064 5.958 5.958 0 01-1.38-2.202 5.196 5.196 0 01-.333-1.589 6.915 6.915 0 01.188-2.132c.45-1.484 1.309-2.648 2.577-3.493.282-.188.55-.334.802-.438.286-.12.573-.22.861-.304a.129.129 0 00.087-.087A6.016 6.016 0 015.635 2.31C6.315 1.464 7.132.846 8.086.457zm-.804 7.85a.848.848 0 00-1.473.842l1.694 2.965-1.688 2.848a.849.849 0 001.46.864l1.94-3.272a.849.849 0 00.007-.854l-1.94-3.393zm5.446 6.24a.849.849 0 000 1.695h4.848a.849.849 0 000-1.696h-4.848z",
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
      return provider[@"label"] ?: @"Usage";
    }
  }
  return @"Usage";
}

/// Add a row to the scrolling content area under the previous one.
- (void)addRow:(NSView *)row height:(CGFloat)height previous:(NSView **)previous topGap:(CGFloat)gap {
  row.translatesAutoresizingMaskIntoConstraints = NO;
  [self.content addSubview:row];
  NSMutableArray *constraints = [NSMutableArray array];
  [constraints addObject:[row.leadingAnchor constraintEqualToAnchor:self.content.leadingAnchor constant:16]];
  [constraints addObject:[row.trailingAnchor constraintEqualToAnchor:self.content.trailingAnchor constant:-16]];
  [constraints addObject:[row.heightAnchor constraintEqualToConstant:height]];
  if (*previous == nil) {
    [constraints addObject:[row.topAnchor constraintEqualToAnchor:self.content.topAnchor constant:10]];
  } else {
    [constraints addObject:[row.topAnchor constraintEqualToAnchor:(*previous).bottomAnchor constant:gap]];
  }
  [NSLayoutConstraint activateConstraints:constraints];
  *previous = row;
}

- (void)renderUsage {
  [self clearHolder:self.headerHolder];
  [self clearHolder:self.footerHolder];
  for (NSView *view in [self.content.subviews copy]) {
    [view removeFromSuperview];
  }

  NSString *active = self.state[@"active"] ?: @"";
  NSDictionary *result = self.state[@"results"][active] ?: @{};

  [self pinChrome:[self makeHeaderRowWithTitle:[self titleForActiveProvider]] inHolder:self.headerHolder];
  [self pinChrome:[self makeFooterRow] inHolder:self.footerHolder];

  NSString *updatedAt = self.state[@"updated_at"];
  NSString *updatedText = @"Waiting for data";
  if ([updatedAt isKindOfClass:[NSString class]] && updatedAt.length > 0) {
    updatedText = [NSString stringWithFormat:@"Updated %@", updatedAt];
  }

  NSView *previous = nil;
  NSString *error = result[@"error"];
  if ([error isKindOfClass:[NSString class]] && error.length > 0) {
    NSTextField *errLabel =
        [self label:error size:12 weight:NSFontWeightRegular color:[NSColor secondaryLabelColor]];
    errLabel.maximumNumberOfLines = 0;
    [self addRow:errLabel height:44 previous:&previous topGap:12];
  } else {
    NSTextField *updated =
        [self label:updatedText size:11 weight:NSFontWeightRegular color:[NSColor secondaryLabelColor]];
    [self addRow:updated height:16 previous:&previous topGap:4];

    NSArray *meters = result[@"meters"] ?: @[];
    NSView *card = nil;     // current account card
    NSView *cardLast = nil; // last row inside it
    for (NSDictionary *meter in meters) {
      NSString *key = meter[@"key"];
      if ([key isKindOfClass:[NSString class]] && [self.hiddenHomes containsObject:key]) {
        continue; // unticked in settings: hidden locally, no refetch needed
      }
      NSString *group = meter[@"group"];
      BOOL grouped = [group isKindOfClass:[NSString class]] && group.length > 0;
      if (grouped && (card == nil || ![group isEqualToString:card.identifier])) {
        if (card != nil && cardLast != nil) {
          [NSLayoutConstraint activateConstraints:@[
            [cardLast.bottomAnchor constraintEqualToAnchor:card.bottomAnchor constant:-6],
          ]];
        }
        card = [self cardViewWithTitle:group key:key];
        card.identifier = group;
        cardLast = [card viewWithTag:1]; // the title label
        [self addCard:card previous:&previous];
      }
      if (grouped) {
        NSView *row = [self compactMeterRow:meter];
        row.translatesAutoresizingMaskIntoConstraints = NO;
        [card addSubview:row];
        [NSLayoutConstraint activateConstraints:@[
          [row.leadingAnchor constraintEqualToAnchor:card.leadingAnchor constant:9],
          [row.trailingAnchor constraintEqualToAnchor:card.trailingAnchor constant:-9],
          [row.topAnchor constraintEqualToAnchor:cardLast.bottomAnchor constant:4],
          [row.heightAnchor constraintEqualToConstant:20],
        ]];
        cardLast = row;
      } else {
        [self addRow:[self meterRow:meter] height:42 previous:&previous topGap:5];
        card = nil;
      }
    }
    if (card != nil && cardLast != nil) {
      [NSLayoutConstraint activateConstraints:@[
        [cardLast.bottomAnchor constraintEqualToAnchor:card.bottomAnchor constant:-6],
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
    [constraints addObject:[card.topAnchor constraintEqualToAnchor:self.content.topAnchor constant:6]];
  } else {
    [constraints addObject:[card.topAnchor constraintEqualToAnchor:(*previous).bottomAnchor constant:7]];
  }
  [NSLayoutConstraint activateConstraints:constraints];
  *previous = card;
}

/// "/Users/x/.codex2" -> "~/.codex2".
- (NSString *)abbreviateHome:(NSString *)path {
  if (![path isKindOfClass:[NSString class]] || path.length == 0) {
    return @"";
  }
  NSString *home = NSHomeDirectory();
  if ([path hasPrefix:home]) {
    return [@"~" stringByAppendingString:[path substringFromIndex:home.length]];
  }
  return path;
}

/// One card per account: rounded panel, email/plan on the left, the CODEX_HOME
/// on the right — so several logins never blur into one list.
- (NSView *)cardViewWithTitle:(NSString *)title key:(NSString *)key {
  NSView *card = [[NSView alloc] initWithFrame:NSZeroRect];
  card.wantsLayer = YES;
  card.layer.cornerRadius = 6;
  card.layer.backgroundColor = [[NSColor labelColor] colorWithAlphaComponent:0.05].CGColor;

  NSTextField *titleLabel =
      [self label:title size:11 weight:NSFontWeightSemibold color:[NSColor labelColor]];
  titleLabel.tag = 1;
  titleLabel.toolTip = title;
  [titleLabel setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                       forOrientation:NSLayoutConstraintOrientationHorizontal];
  [card addSubview:titleLabel];

  NSString *home = [self abbreviateHome:key];
  NSTextField *homeLabel =
      [self label:home size:10 weight:NSFontWeightRegular color:[NSColor tertiaryLabelColor]];
  homeLabel.toolTip = key;
  [card addSubview:homeLabel];

  [NSLayoutConstraint activateConstraints:@[
    [titleLabel.leadingAnchor constraintEqualToAnchor:card.leadingAnchor constant:9],
    [titleLabel.topAnchor constraintEqualToAnchor:card.topAnchor constant:6],
    [homeLabel.trailingAnchor constraintEqualToAnchor:card.trailingAnchor constant:-9],
    [homeLabel.firstBaselineAnchor constraintEqualToAnchor:titleLabel.firstBaselineAnchor],
    [homeLabel.leadingAnchor constraintGreaterThanOrEqualToAnchor:titleLabel.trailingAnchor
                                                         constant:6],
  ]];
  return card;
}

/// Compact meter for use inside a card: one text line over a thin bar.
- (NSView *)compactMeterRow:(NSDictionary *)meter {
  NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];

  NSString *labelText = meter[@"label"] ?: @"Usage";
  NSNumber *percentNumber = meter[@"percent"] ?: @0;
  NSString *detailText = meter[@"detail"] ?: @"";
  double percent = MAX(0, MIN(100, percentNumber.doubleValue));

  NSTextField *label = [self label:[NSString stringWithFormat:@"%@  %.0f%%", labelText, percent]
                              size:11 weight:NSFontWeightSemibold color:[NSColor labelColor]];
  [label setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                  forOrientation:NSLayoutConstraintOrientationHorizontal];
  NSTextField *detail =
      [self label:detailText size:10 weight:NSFontWeightRegular color:[NSColor secondaryLabelColor]];
  [detail setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                   forOrientation:NSLayoutConstraintOrientationHorizontal];
  NSProgressIndicator *progress = [[NSProgressIndicator alloc] initWithFrame:NSZeroRect];
  progress.indeterminate = NO;
  progress.minValue = 0;
  progress.maxValue = 100;
  progress.doubleValue = percent;
  progress.controlSize = NSControlSizeSmall;

  label.translatesAutoresizingMaskIntoConstraints = NO;
  detail.translatesAutoresizingMaskIntoConstraints = NO;
  progress.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:label];
  [row addSubview:detail];
  [row addSubview:progress];

  [NSLayoutConstraint activateConstraints:@[
    [label.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
    [label.topAnchor constraintEqualToAnchor:row.topAnchor],
    [detail.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
    [detail.firstBaselineAnchor constraintEqualToAnchor:label.firstBaselineAnchor],
    [detail.leadingAnchor constraintGreaterThanOrEqualToAnchor:label.trailingAnchor constant:6],
    [progress.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
    [progress.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
    [progress.bottomAnchor constraintEqualToAnchor:row.bottomAnchor],
    [progress.heightAnchor constraintEqualToConstant:5],
  ]];
  return row;
}

/// Account heading: "email · Plan · ~/.codex" above that account's meters.
- (NSView *)groupRowWithTitle:(NSString *)title first:(BOOL)first {
  NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];

  NSTextField *label = [self label:title size:11 weight:NSFontWeightSemibold
                             color:[NSColor secondaryLabelColor]];
  label.toolTip = title;
  [label setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                  forOrientation:NSLayoutConstraintOrientationHorizontal];
  label.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:label];

  NSMutableArray *constraints = [NSMutableArray array];
  [constraints addObject:[label.leadingAnchor constraintEqualToAnchor:row.leadingAnchor]];
  [constraints addObject:[label.trailingAnchor constraintEqualToAnchor:row.trailingAnchor]];
  [constraints addObject:[label.bottomAnchor constraintEqualToAnchor:row.bottomAnchor]];

  // Hairline above every group but the first, to separate accounts.
  if (!first) {
    NSView *line = [[NSView alloc] initWithFrame:NSZeroRect];
    line.translatesAutoresizingMaskIntoConstraints = NO;
    line.wantsLayer = YES;
    line.layer.backgroundColor =
        [[NSColor separatorColor] colorWithAlphaComponent:0.5].CGColor;
    [row addSubview:line];
    [constraints addObjectsFromArray:@[
      [line.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
      [line.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
      [line.topAnchor constraintEqualToAnchor:row.topAnchor],
      [line.heightAnchor constraintEqualToConstant:1],
    ]];
  }
  [NSLayoutConstraint activateConstraints:constraints];
  return row;
}

- (NSView *)meterRow:(NSDictionary *)meter {
  NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];
  // No fill — flat, linear, separated by whitespace alone.

  NSString *labelText = meter[@"label"] ?: @"Usage";
  NSNumber *percentNumber = meter[@"percent"] ?: @0;
  NSString *detailText = meter[@"detail"] ?: @"";
  double percent = MAX(0, MIN(100, percentNumber.doubleValue));

  NSTextField *label = [self label:[NSString stringWithFormat:@"%@  %.0f%%", labelText, percent]
                              size:12 weight:NSFontWeightSemibold color:[NSColor labelColor]];
  NSTextField *detail = [self label:detailText size:11 weight:NSFontWeightRegular color:[NSColor secondaryLabelColor]];
  [detail setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                   forOrientation:NSLayoutConstraintOrientationHorizontal];
  NSProgressIndicator *progress = [[NSProgressIndicator alloc] initWithFrame:NSZeroRect];
  progress.indeterminate = NO;
  progress.minValue = 0;
  progress.maxValue = 100;
  progress.doubleValue = percent;
  progress.controlSize = NSControlSizeSmall;

  label.translatesAutoresizingMaskIntoConstraints = NO;
  detail.translatesAutoresizingMaskIntoConstraints = NO;
  progress.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:label];
  [row addSubview:detail];
  [row addSubview:progress];

  [NSLayoutConstraint activateConstraints:@[
    [label.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
    [label.topAnchor constraintEqualToAnchor:row.topAnchor constant:8],
    [detail.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
    [detail.centerYAnchor constraintEqualToAnchor:label.centerYAnchor],
    [detail.leadingAnchor constraintGreaterThanOrEqualToAnchor:label.trailingAnchor constant:8],
    [progress.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
    [progress.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
    [progress.topAnchor constraintEqualToAnchor:label.bottomAnchor constant:7],
    [progress.heightAnchor constraintEqualToConstant:8],
  ]];
  return row;
}

// ---------- settings view ----------

- (void)renderSettings {
  [self clearHolder:self.headerHolder];
  [self clearHolder:self.footerHolder];
  [self.fieldInputs removeAllObjects];
  [self.codexAccountRows removeAllObjects];
  self.showSpendToggle = nil;
  self.showTodayToggle = nil;
  self.usedRemainingControl = nil;
  for (NSView *view in [self.content.subviews copy]) {
    [view removeFromSuperview];
  }

  NSString *active = self.state[@"active"] ?: @"";
  [self pinChrome:[self makeHeaderRowWithTitle:[self titleForActiveProvider]] inHolder:self.headerHolder];
  [self pinChrome:[self makeFooterRow] inHolder:self.footerHolder];

  NSView *previous = nil;
  if ([active isEqualToString:@"codex"]) {
    [self renderCodexAccountsInto:&previous];
    // Off by default: the workspace spend cap is 0 credits on most plans and
    // only adds a permanently-red row.
    NSButton *showSpend = [NSButton checkboxWithTitle:@"Show spend limit"
                                               target:self
                                               action:@selector(settingsEdited:)];
    showSpend.state = [self.state[@"codex_show_spend"] boolValue] ? NSControlStateValueOn
                                                                  : NSControlStateValueOff;
    showSpend.controlSize = NSControlSizeMini;
    showSpend.font = [NSFont systemFontOfSize:MAX(kMinFontSize, 11 + kPanelFontDelta)];
    showSpend.toolTip = @"Show each workspace's spend-control meter";
    [self addRow:showSpend height:20 previous:&previous topGap:10];
    self.showSpendToggle = showSpend;

    // "Today" is computed from the SQLite history, so it is opt-in like spend.
    NSButton *showToday = [NSButton checkboxWithTitle:@"Show today's usage"
                                               target:self
                                               action:@selector(settingsEdited:)];
    showToday.state = [self.state[@"codex_show_today"] boolValue] ? NSControlStateValueOn
                                                                  : NSControlStateValueOff;
    showToday.controlSize = NSControlSizeMini;
    showToday.font = [NSFont systemFontOfSize:MAX(kMinFontSize, 11 + kPanelFontDelta)];
    showToday.toolTip = @"Add a per-account row with quota burned since midnight";
    [self addRow:showToday height:20 previous:&previous topGap:6];
    self.showTodayToggle = showToday;

    // Read the window meters as quota used or quota left.
    NSView *modeRow = [[NSView alloc] initWithFrame:NSZeroRect];
    NSTextField *modeLabel =
        [self label:@"Meters" size:11 weight:NSFontWeightMedium color:[NSColor labelColor]];
    [modeRow addSubview:modeLabel];
    NSSegmentedControl *mode =
        [NSSegmentedControl segmentedControlWithLabels:@[ @"Used", @"Remaining" ]
                                          trackingMode:NSSegmentSwitchTrackingSelectOne
                                                target:self
                                                action:@selector(settingsEdited:)];
    mode.controlSize = NSControlSizeMini;
    mode.selectedSegment = [self.state[@"codex_show_remaining"] boolValue] ? 1 : 0;
    mode.toolTip = @"Show quota used or quota left in the panel";
    mode.translatesAutoresizingMaskIntoConstraints = NO;
    [modeRow addSubview:mode];
    [NSLayoutConstraint activateConstraints:@[
      [modeLabel.leadingAnchor constraintEqualToAnchor:modeRow.leadingAnchor],
      [modeLabel.centerYAnchor constraintEqualToAnchor:modeRow.centerYAnchor],
      [mode.trailingAnchor constraintEqualToAnchor:modeRow.trailingAnchor],
      [mode.centerYAnchor constraintEqualToAnchor:modeRow.centerYAnchor],
      [mode.leadingAnchor constraintGreaterThanOrEqualToAnchor:modeLabel.trailingAnchor constant:8],
    ]];
    [self addRow:modeRow height:22 previous:&previous topGap:8];
    self.usedRemainingControl = mode;
  } else {
    NSDictionary *creds = self.state[@"credentials"][active] ?: @{};
    NSArray *fields = [self fieldsForProvider:active];
    for (NSDictionary *fieldDef in fields) {
      NSString *field = fieldDef[@"field"];
      NSString *labelText = fieldDef[@"label"];
      BOOL secure = [fieldDef[@"secure"] boolValue];

      NSTextField *fieldLabel =
          [self label:labelText size:11 weight:NSFontWeightMedium color:[NSColor labelColor]];
      [self addRow:fieldLabel height:14 previous:&previous topGap:12];

      NSTextField *input = secure ? [[NSSecureTextField alloc] initWithFrame:NSZeroRect]
                                  : [[NSTextField alloc] initWithFrame:NSZeroRect];
      input.stringValue = creds[field] ?: @"";
      input.placeholderString = labelText;
      input.delegate = self;
      input.controlSize = NSControlSizeMini;
      input.font = [NSFont systemFontOfSize:MAX(kMinFontSize, 11 + kPanelFontDelta)];
      [self addRow:input height:22 previous:&previous topGap:4];
      self.fieldInputs[field] = input;
    }
  }

  // Codex accounts come from disk, so the extra button re-scans ~/.codex*.
  NSView *lastButton = nil;
  if ([active isEqualToString:@"codex"]) {
    NSButton *rescan = [NSButton buttonWithTitle:@"Rescan ~/.codex*"
                                          target:self
                                          action:@selector(rescanClicked:)];
    rescan.controlSize = NSControlSizeMini;
    [self addRow:rescan height:24 previous:&previous topGap:14];
    lastButton = rescan;
  }

  NSButton *saveButton = [NSButton buttonWithTitle:@"Save & Refresh"
                                             target:self
                                            action:@selector(saveClicked:)];
  saveButton.controlSize = NSControlSizeMini;
  [self addRow:saveButton height:24 previous:&previous topGap:(lastButton == nil ? 14 : 8)];

  if (previous != nil) {
    [NSLayoutConstraint activateConstraints:@[
      [previous.bottomAnchor constraintEqualToAnchor:self.content.bottomAnchor constant:-10],
    ]];
  }
}

/// Editor for the Codex account list: enable toggle, custom name, and the
/// identity read from each home's auth.json.
- (void)renderCodexAccountsInto:(NSView **)previous {
  NSArray *accounts = self.state[@"codex_accounts"] ?: @[];

  NSTextField *hint = [self label:@"ChatGPT logins · read-only"
                             size:11 weight:NSFontWeightRegular color:[NSColor secondaryLabelColor]];
  [hint setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                 forOrientation:NSLayoutConstraintOrientationHorizontal];
  [self addRow:hint height:14 previous:previous topGap:6];

  if (accounts.count == 0) {
    NSTextField *empty = [self label:@"No ~/.codex* logins found"
                               size:12 weight:NSFontWeightRegular color:[NSColor secondaryLabelColor]];
    [self addRow:empty height:20 previous:previous topGap:10];
    return;
  }

  for (NSDictionary *account in accounts) {
    NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];

    NSButton *toggle = [NSButton checkboxWithTitle:@""
                                            target:self
                                            action:@selector(accountToggled:)];
    NSString *home = account[@"home"] ?: @"";
    toggle.identifier = home;
    BOOL shown = [account[@"enabled"] boolValue] && ![self.hiddenHomes containsObject:home];
    toggle.state = shown ? NSControlStateValueOn : NSControlStateValueOff;
    toggle.toolTip = @"Show this account in the usage panel";
    toggle.translatesAutoresizingMaskIntoConstraints = NO;
    [row addSubview:toggle];

    NSTextField *nameField = [[NSTextField alloc] initWithFrame:NSZeroRect];
    nameField.stringValue = account[@"label"] ?: @"";
    nameField.placeholderString = account[@"email"] ?: @"name";
    nameField.controlSize = NSControlSizeMini;
    nameField.delegate = self;
    nameField.font = [NSFont systemFontOfSize:MAX(kMinFontSize, 11 + kPanelFontDelta)];
    nameField.translatesAutoresizingMaskIntoConstraints = NO;
    [row addSubview:nameField];

    // Subtitle: email · Plan · home, or the reason the login is unusable.
    // Home first: with several homes for one account it is the only thing that
    // tells the rows apart, and the panel is narrow.
    NSMutableArray *parts = [NSMutableArray array];
    [parts addObject:account[@"home_display"] ?: @""];
    if ([account[@"plan"] length] > 0) {
      [parts addObject:account[@"plan"]];
    }
    NSString *subtitle = [parts componentsJoinedByString:@" · "];
    if ([account[@"duplicate_of"] length] > 0) {
      subtitle = [subtitle stringByAppendingFormat:@"  (same as %@)", account[@"duplicate_of"]];
    }
    NSString *problem = account[@"error"];
    BOOL hasProblem = [problem isKindOfClass:[NSString class]] && problem.length > 0;
    NSTextField *subtitleField =
        [self label:(hasProblem ? problem : subtitle)
               size:10 weight:NSFontWeightRegular
              color:(hasProblem ? [NSColor systemOrangeColor] : [NSColor secondaryLabelColor])];
    subtitleField.toolTip = subtitle;
    [subtitleField setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                            forOrientation:NSLayoutConstraintOrientationHorizontal];
    subtitleField.translatesAutoresizingMaskIntoConstraints = NO;
    [row addSubview:subtitleField];

    [NSLayoutConstraint activateConstraints:@[
      [toggle.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
      [toggle.topAnchor constraintEqualToAnchor:row.topAnchor constant:2],
      [toggle.widthAnchor constraintEqualToConstant:18],
      [nameField.leadingAnchor constraintEqualToAnchor:toggle.trailingAnchor constant:6],
      [nameField.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
      [nameField.centerYAnchor constraintEqualToAnchor:toggle.centerYAnchor],
      [subtitleField.leadingAnchor constraintEqualToAnchor:nameField.leadingAnchor],
      [subtitleField.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
      [subtitleField.topAnchor constraintEqualToAnchor:nameField.bottomAnchor constant:3],
    ]];

    [self addRow:row height:44 previous:previous topGap:8];
    [self.codexAccountRows addObject:@{
      @"home" : account[@"home"] ?: @"",
      @"checkbox" : toggle,
      @"labelField" : nameField
    }];
  }
}

- (NSArray *)fieldsForProvider:(NSString *)provider {
  if ([provider isEqualToString:@"opencode"]) {
    return @[
      @{@"field": @"workspace_id", @"label": @"Workspace ID", @"secure": @NO},
      @{@"field": @"auth_cookie", @"label": @"Auth Cookie", @"secure": @YES},
    ];
  }
  if ([provider isEqualToString:@"codex"]) {
    return @[];
  }
  return @[ @{@"field": @"api_key", @"label": @"API Key", @"secure": @YES} ];
}

// ---------- shared subviews ----------

- (NSView *)makeHeaderRowWithTitle:(NSString *)titleText {
  NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];
  row.translatesAutoresizingMaskIntoConstraints = NO;

  NSTextField *title =
      [self label:titleText size:15 weight:NSFontWeightBold color:[NSColor labelColor]];
  title.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:title];

  NSButton *gear =
      [NSButton buttonWithTitle:@"" target:self action:@selector(toggleSettings:)];
  // Toggle icon: gear when on usage view, chart when on settings view.
  NSString *symbol = self.settingsVisible ? @"chart.bar.fill" : @"gearshape";
  gear.image = [NSImage imageWithSystemSymbolName:symbol accessibilityDescription:symbol];
  gear.imagePosition = NSImageOnly;
  gear.bordered = NO;
  gear.contentTintColor = [NSColor secondaryLabelColor];
  gear.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:gear];

  [NSLayoutConstraint activateConstraints:@[
    [title.leadingAnchor constraintEqualToAnchor:row.leadingAnchor constant:16],
    [title.topAnchor constraintEqualToAnchor:row.topAnchor constant:14],
    [gear.trailingAnchor constraintEqualToAnchor:row.trailingAnchor constant:-12],
    [gear.centerYAnchor constraintEqualToAnchor:title.centerYAnchor],
    [gear.widthAnchor constraintEqualToConstant:24],
    [gear.heightAnchor constraintEqualToConstant:24],
    [row.heightAnchor constraintGreaterThanOrEqualToConstant:44],
  ]];
  return row;
}

- (NSView *)makeFooterRow {
  NSView *row = [[NSView alloc] initWithFrame:NSZeroRect];
  row.translatesAutoresizingMaskIntoConstraints = NO;

  // Hairline top border separates the footer from content above.
  NSView *topLine = [[NSView alloc] initWithFrame:NSZeroRect];
  topLine.translatesAutoresizingMaskIntoConstraints = NO;
  topLine.wantsLayer = YES;
  topLine.layer.backgroundColor = [NSColor separatorColor].CGColor;
  [row addSubview:topLine];

  NSButton *refresh =
      [NSButton buttonWithTitle:@"Refresh" target:self action:@selector(refreshClicked:)];
  refresh.bezelStyle = NSBezelStyleAccessoryBar;
  refresh.controlSize = NSControlSizeMini;
  refresh.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:refresh];

  NSButton *quit = [NSButton buttonWithTitle:@"Quit" target:self action:@selector(quitClicked:)];
  quit.bezelStyle = NSBezelStyleAccessoryBar;
  quit.controlSize = NSControlSizeMini;
  quit.translatesAutoresizingMaskIntoConstraints = NO;
  [row addSubview:quit];

  [NSLayoutConstraint activateConstraints:@[
    [topLine.leadingAnchor constraintEqualToAnchor:row.leadingAnchor],
    [topLine.trailingAnchor constraintEqualToAnchor:row.trailingAnchor],
    [topLine.topAnchor constraintEqualToAnchor:row.topAnchor],
    [topLine.heightAnchor constraintEqualToConstant:1],
    [refresh.leadingAnchor constraintEqualToAnchor:row.leadingAnchor constant:12],
    [refresh.topAnchor constraintEqualToAnchor:topLine.bottomAnchor constant:6],
    [quit.trailingAnchor constraintEqualToAnchor:row.trailingAnchor constant:-12],
    [quit.centerYAnchor constraintEqualToAnchor:refresh.centerYAnchor],
    [row.heightAnchor constraintEqualToConstant:30],
  ]];
  return row;
}

- (NSTextField *)label:(NSString *)text size:(CGFloat)size weight:(NSFontWeight)weight color:(NSColor *)color {
  NSTextField *label = [NSTextField labelWithString:text ?: @""];
  label.font = [NSFont systemFontOfSize:MAX(kMinFontSize, size + kPanelFontDelta) weight:weight];
  label.textColor = color;
  label.lineBreakMode = NSLineBreakByTruncatingTail;
  label.translatesAutoresizingMaskIntoConstraints = NO;
  return label;
}

// ---------- actions ----------

- (void)providerClicked:(NSButton *)sender {
  NSString *provider = sender.identifier;
  if (provider.length == 0) {
    return;
  }
  // Switching provider returns to the usage view.
  self.settingsVisible = NO;
  goProviderSelected(provider.UTF8String);
}

- (void)toggleSettings:(id)sender {
  self.settingsVisible = !self.settingsVisible;
  self.settingsDirty = NO;
  [self renderAll];
}

- (void)refreshClicked:(id)sender {
  goRefreshRequested();
}

- (void)quitClicked:(id)sender {
  goQuitRequested();
  [NSApp terminate:nil];
}

- (void)rescanClicked:(id)sender {
  goRescanCodexAccounts();
  // The refreshed state re-renders the list (asynchronously).
}

- (void)saveClicked:(id)sender {
  NSString *active = self.state[@"active"] ?: @"";
  if ([active isEqualToString:@"codex"]) {
    NSMutableArray *payload = [NSMutableArray array];
    for (NSDictionary *row in self.codexAccountRows) {
      NSString *home = row[@"home"];
      if (home.length == 0) {
        continue;
      }
      NSButton *checkbox = row[@"checkbox"];
      NSTextField *labelField = row[@"labelField"];
      [payload addObject:@{
        @"home" : home,
        @"label" : labelField.stringValue ?: @"",
        @"enabled" : @(checkbox.state == NSControlStateValueOn),
      }];
    }
    NSMutableDictionary *settings = [NSMutableDictionary dictionary];
    settings[@"accounts"] = payload;
    settings[@"show_spend"] = @(self.showSpendToggle.state == NSControlStateValueOn);
    if (self.showTodayToggle != nil) {
      settings[@"show_today"] = @(self.showTodayToggle.state == NSControlStateValueOn);
    }
    if (self.usedRemainingControl != nil) {
      settings[@"show_remaining"] = @(self.usedRemainingControl.selectedSegment == 1);
    }
    NSData *data = [NSJSONSerialization dataWithJSONObject:settings options:0 error:nil];
    if (data != nil) {
      NSString *json = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
      goSaveCodexAccounts(json.UTF8String);
    }
    self.settingsDirty = NO;
    self.settingsVisible = NO;
    return;
  }

  NSArray *fields = [self fieldsForProvider:active];
  for (NSDictionary *fieldDef in fields) {
    NSString *field = fieldDef[@"field"];
    NSTextField *input = self.fieldInputs[field];
    if (input == nil) {
      continue;
    }
    goSaveCredentials(active.UTF8String, field.UTF8String, input.stringValue.UTF8String);
  }
  self.settingsDirty = NO;
  self.settingsVisible = NO;
}

@end

// ---------------------------------------------------------------------------
// AppDelegate: owns the NSStatusItem and the NSPopover.
// ---------------------------------------------------------------------------

@interface OCGAppDelegate : NSObject <NSApplicationDelegate>
@property(strong) NSStatusItem *statusItem;
@property(strong) NSPopover *popover;
@property(strong) UsagePanelController *controller;
@end

@implementation OCGAppDelegate

- (void)applicationDidFinishLaunching:(NSNotification *)note {
  [[NSProcessInfo processInfo] disableAutomaticTermination:@"OCG menu bar monitor"];

  self.statusItem =
      [[NSStatusBar systemStatusBar] statusItemWithLength:NSVariableStatusItemLength];
  self.statusItem.button.target = self;
  self.statusItem.button.action = @selector(togglePopover:);
  // No icon until the first refresh pushes a template gauge icon.

  self.controller = [[UsagePanelController alloc] init];
  self.popover = [[NSPopover alloc] init];
  self.popover.contentViewController = self.controller;
  self.popover.behavior = NSPopoverBehaviorTransient;
  self.popover.animates = YES;

  // Hand off to the Rust data layer — starts the background refresh loop.
  goOnReady();

  // Debug hook: `OCG_AUTOOPEN=1` shows the popover shortly after launch so the
  // panel can be inspected without hunting for the status item.
  if (getenv("OCG_AUTOOPEN") != NULL) {
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

- (void)setStatusIconBytes:(const void *)bytes length:(size_t)length {
  NSData *data = [NSData dataWithBytes:bytes length:length];
  NSImage *image = [[NSImage alloc] initWithData:data];
  if (image == nil) {
    return;
  }
  // Template image: AppKit tints it to match the menu bar (dark/light).
  image.template = YES;
  dispatch_async(dispatch_get_main_queue(), ^{
    self.statusItem.button.image = image;
    self.statusItem.button.imagePosition =
        (self.statusItem.button.title.length > 0 ? NSImageLeading : NSImageOnly);
  });
}

/// Short numeric badge next to the icon; red/amber once an account runs hot.
- (void)setStatusTitleText:(NSString *)title {
  dispatch_async(dispatch_get_main_queue(), ^{
    NSStatusBarButton *button = self.statusItem.button;
    // Debug aid: OCG_STATUS_TITLE forces a fixed badge so the item is easy to
    // spot in screenshots.
    const char *forced = getenv("OCG_STATUS_TITLE");
    NSString *text = forced != NULL ? [NSString stringWithUTF8String:forced] : (title ?: @"");
    button.font = [NSFont monospacedDigitSystemFontOfSize:11 weight:NSFontWeightMedium];
    int percent = text.intValue;
    // Colour via an attributed title, never contentTintColor: on macOS 26 the
    // latter makes the whole status item stop drawing (which used to hide the
    // badge exactly when usage went critical).
    NSColor *tint = nil;
    if (percent >= 80) {
      tint = [NSColor systemRedColor];
    } else if (percent >= 50) {
      tint = [NSColor systemOrangeColor];
    }
    NSMutableDictionary *attributes = [NSMutableDictionary dictionary];
    attributes[NSFontAttributeName] = button.font;
    if (tint != nil) {
      attributes[NSForegroundColorAttributeName] = tint;
    }
    button.attributedTitle = [[NSAttributedString alloc] initWithString:text
                                                            attributes:attributes];
    button.imagePosition = (text.length > 0 ? NSImageLeading : NSImageOnly);
    if (forced != NULL) {
      NSRect frame = button.window.frame;
      fprintf(stderr, "[ocg] status item: x=%.0f y=%.0f w=%.0f h=%.0f | button %.0fx%.0f | visible=%d\n",
              frame.origin.x, frame.origin.y, frame.size.width, frame.size.height,
              button.frame.size.width, button.frame.size.height, self.statusItem.isVisible);
    }
  });
}

- (void)setTooltip:(NSString *)tooltip {
  dispatch_async(dispatch_get_main_queue(), ^{
    self.statusItem.button.toolTip = tooltip;
  });
}

- (void)updateWithStateJSON:(NSString *)json {
  dispatch_async(dispatch_get_main_queue(), ^{
    [self.controller updateWithStateJSON:json];
    [self writeSnapshotIfRequested];
  });
}

/// Debug hook: `OCG_SNAPSHOT=/tmp/panel.png` renders the panel off-screen and
/// writes a PNG, so the layout can be inspected without hunting for the status
/// item. `OCG_SNAPSHOT_SETTINGS=1` captures the settings pane instead.
- (void)writeSnapshotIfRequested {
  static BOOL written = NO;
  const char *path = getenv("OCG_SNAPSHOT");
  if (written || path == NULL) {
    return;
  }
  written = YES;

  NSWindow *offscreen = [[NSWindow alloc]
      initWithContentRect:NSMakeRect(0, 0, 260, 546)
                styleMask:NSWindowStyleMaskBorderless
                  backing:NSBackingStoreBuffered
                    defer:NO];
  offscreen.contentViewController = self.controller;
  if (getenv("OCG_SNAPSHOT_SETTINGS") != NULL) {
    self.controller.settingsVisible = YES;
  }
  // viewWillAppear never fires for an off-screen window, so render by hand.
  self.controller.preferredContentSize = NSMakeSize(260, 546);
  [self.controller.view setFrameSize:NSMakeSize(260, 546)];
  [self.controller renderAll];
  [self.controller.view layoutSubtreeIfNeeded];

  NSRect bounds = self.controller.view.bounds;
  NSBitmapImageRep *rep = [self.controller.view bitmapImageRepForCachingDisplayInRect:bounds];
  [self.controller.view cacheDisplayInRect:bounds toBitmapImageRep:rep];
  NSData *png = [rep representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
  [png writeToFile:[NSString stringWithUTF8String:path] atomically:YES];
  fprintf(stderr, "[ocg] panel snapshot written to %s (%.0fx%.0f)\n", path, bounds.size.width,
          bounds.size.height);
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

void setStatusIcon(const void *bytes, size_t length) {
  @autoreleasepool {
    [ocgAppDelegate setStatusIconBytes:bytes length:length];
  }
}

void setStatusTitle(const char *title) {
  @autoreleasepool {
    [ocgAppDelegate setStatusTitleText:[NSString stringWithUTF8String:title ?: ""]];
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
