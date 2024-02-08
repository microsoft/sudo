# Sudo for Windows Contributor's Guide

**Sudo for Windows is not currently open source**. We're still in the process of
crossing our "t"'s and dotting our "i"s. Stay tuned for more updates. In the
meantime, we are still accepting issues, feature requests, and contributions to
the [`sudo.ps1`] script.

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

### Repo Bot

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
