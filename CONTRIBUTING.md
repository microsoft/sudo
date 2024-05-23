# Sudo for Windows Contributor's Guide

Below is our guidance for how to report issues, propose new features, and submit
contributions via Pull Requests (PRs). Our philosophy is heavily based around
understanding the problem and scenarios first, this is why we follow this
pattern before work has started.

1. There is an issue
2. There has been a conversation
3. There is agreement on the problem, the fit for Sudo for Windows, and the
   solution to the problem (implementation)

## Filing an issue

Please follow this simple rule to help us eliminate any unnecessary wasted
effort & frustration, and ensure an efficient and effective use of everyone's
time - yours, ours, and other community members':

> 👉 If you have a question, think you've discovered an issue, would like to
> propose a new feature, etc., then find/file an issue **BEFORE** starting work
> to fix/implement it.

When requesting new features / enhancements, understanding problem and scenario
around it is extremely important. Having additional evidence, data, tweets, blog
posts, research, ... anything is extremely helpful.  This information provides
context to the scenario that may otherwise be lost.

* Don't know whether you're reporting an issue or requesting a feature? **File an issue**
* Have a question that you don't see answered in docs, videos, etc.? **File an issue**
* Want to know if we're planning on building a particular feature? **File an issue**
* Got a great idea for a new feature? **File an issue**
* Don't understand how to do something? **File an issue**
* Found an existing issue that describes yours? **Great!** upvote and add
  additional commentary / info / repro-steps / etc.

A quick search before filing an issue also could be helpful. It is likely
someone else has found the problem you're seeing, and someone may be working on
or have already contributed a fix!

### Reporting Security Issues

**Please do not report security vulnerabilities through public GitHub issues.**
Instead, please report them to the Microsoft Security Response Center (MSRC).
See [SECURITY.md](./SECURITY.md) for more information.

### How to tell the Sudo team "this is an interesting thing to focus on"

Upvote the original issue by clicking its [+😊] button and hitting 👍 (+1) icon
or a different one. This way allows us to measure how impactful different issues
are compared to others. The issue with comments like "+1", "me too", or similar
is they actually make it harder to have a conversation and harder to quickly
determine trending important requests.

### Localization issues

Please file localization issues, so our internal localization team can identify
and fix them. However we currently don't accept community Pull Requests fixing
localization issues. Localization is handled by the internal Microsoft team
only.

## Contributing code

As you might imagine, shipping something inside of Windows is a complicated
process--doubly so for a security component. There is a lot of validation and
paperwork that we don't really want to subject the community to. We want you to
be able to contribute easily and freely, and to let us deal with the paperwork.
We'll do our best to make sure this process is as seamless as possible.

To support the community in building new feature areas for Sudo for Windows,
we're going to make extensive use of feature flags, to conditionally add new
features to Sudo for Windows.

When contributing to Sudo for Windows, we will treat "bugfixes" and "features"
separately. Bug fixes can be merged into the codebase freely, so long as they
don't majorly change existing behaviors. New features will need to have their
code guarded by feature flag checks before we can accept them as contributions.

As always, filing issues on the repo is the best way to have the team evaluate
if the change you're proposing would be considered a "bug fix" or "feature"
work. We will indicate which is which using the `Issue-Bug` and
`Issue-Feature`/`Issue-Task` labels. These labels are intended to be informative,
and may change throughout the lifecycle of a discussion.

We'll be grouping sets of feature flags into different "branding"s throughout
the project. These brandings are as follows:
* **"Inbox"**: These are features that are included in the version of sudo that
  ships with Windows itself.
* **"Stable"**: Features that ship in stable versions of sudo released here on
  GitHub and on WinGet.
* **"Dev"**: The least stable features, which are only built in local development
  builds. These are for work-in-progress features, that aren't quite ready for
  public consumption

All new features should be added under the "Dev" branding first. The core team
will then be responsible for moving those features into the appropriate branding
as we get internal signoffs. This will allow features to be worked on
continually in the open, while we slowly roll them into the OS product. We
unfortunately cannot provide timelines for when features will be able to move
from Stable into Inbox. Historical data showing that a feature has a track
record of being stable and secure is a great way for us to justify any
particular feature's inclusion into the product.

If you're ready to jump in, head on over to [Building.md](./Building.md) to get
started.

## Repo Bot

The team triages new issues several times a week. During triage, the team uses
labels to categorize, manage, and drive the project workflow.

We employ a bot engine to help us automate common processes within our workflow.

We drive the bot by tagging issues with specific labels which cause the bot
engine to close issues, merge branches, etc. This bot engine helps us keep the
repo clean by automating the process of notifying appropriate parties if/when
information/follow-up is needed, and closing stale issues/PRs after reminders
have remained unanswered for several days.

Therefore, if you do file issues, or create PRs, please keep an eye on your
GitHub notifications. If you do not respond to requests for information, your
issues/PRs may be closed automatically.

## Thank you

Thank you in advance for your contribution!

[`sudo.ps1`]: ./scripts/sudo.ps1
