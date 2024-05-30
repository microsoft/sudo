# ![Sudo for Windows icon](./img/Windows/AppList.targetsize-24.png) Sudo for Windows

Welcome to the repository for [Sudo for Windows][sudo-for-windows] 🥪. Sudo
for Windows allows users to run elevated commands directly from unelevated
terminal windows.

The "Inbox" version of sudo is available for Windows 11 builds 26045 and later. If you're on an Insiders
build with sudo, you can enable it in the Windows Settings app, on the
"Developer Features" page.

Here you can report issues and file feature requests.

## Relationship to `sudo` on Unix/Linux

Everything about permissions and the command line experience is
different between Windows and Linux. This project is not a fork of the Unix/Linux
`sudo` project, nor is it a port of that `sudo` project. Instead, Sudo for
Windows is a Windows-specific implementation of the `sudo` concept.

As the two are entirely different applications, you'll find that certain
elements of the traditional `sudo` experience are not present in Sudo for Windows, and
vice versa. Scripts and documentation that are written for `sudo` may not
be able to be used directly with Sudo for Windows without some modification.

## Documentation

All project documentation is located at
[aka.ms/sudo-docs][sudo-docs]. If you would like to contribute to
the documentation, please submit a pull request on the [Sudo for Windows
Documentation repo][documentation-repo].

## Contributing

Check out [CONTRIBUTING.md](https://github.com/microsoft/sudo/blob/main/CONTRIBUTING.md) for details on how to contribute to this project.

### `sudo.ps1`

In the meantime, you can contribute to the [`sudo.ps1`] script. This script is
meant to be a helper wrapper around `sudo.exe` that provides a more
user-friendly experience for using sudo from PowerShell. This script is located
in the `scripts/` directory.

## Communicating with the Team

The easiest way to communicate with the team is via GitHub issues.

Please file new issues, feature requests and suggestions, but **DO search for
similar open/closed preexisting issues before creating a new issue.**

If you would like to ask a question that you feel doesn't warrant an issue
(yet), try a [discussion thread][discussions]. Those are especially helpful for question &
answer threads. Otherwise, you can reach out to us via your social media
platform of choice:

* Mike Griese, Senior Developer: [@zadjii@mastodon.social](https://mastodon.social/@zadjii)
* Jordi Adoumie, Product Manager: [@joadoumie](https://twitter.com/joadoumie)
* Dustin Howett, Engineering Lead: [@dhowett@mas.to](https://mas.to/@DHowett)
* Clint Rutkas, Lead Product Manager: [@crutkas](https://twitter.com/clintrutkas)

## Code of Conduct

This project has adopted the [Microsoft Open Source Code of
Conduct][conduct-code]. For more information see the [Code of Conduct
FAQ][conduct-FAQ] or contact [opencode@microsoft.com][conduct-email] with any
additional questions or comments.

[conduct-code]: https://opensource.microsoft.com/codeofconduct/
[conduct-FAQ]: https://opensource.microsoft.com/codeofconduct/faq/
[conduct-email]: mailto:opencode@microsoft.com
[`sudo.ps1`]: ./scripts/sudo.ps1
[discussions]: https://github.com/microsoft/sudo/discussions
[sudo-for-windows]: https://aka.ms/sudo
[sudo-docs]: https://aka.ms/sudo-docs
[documentation-repo]: https://github.com/MicrosoftDocs/windows-dev-docs/tree/docs/hub/sudo
