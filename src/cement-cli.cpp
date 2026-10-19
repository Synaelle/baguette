// cement-cli - the old name of chomp-cli, kept so existing scripts and agents
// still work: runs chomp-cli.exe from the same folder with the same arguments
// and returns its exit code. Output goes straight through; a notice on stderr
// says it is a stub that will go away. Remove it in a later build.

#include <windows.h>

#include <string>

int wmain ()
{
	// the command line minus our own program name
	wchar_t const *args = GetCommandLineW ();
	bool quoted         = *args == L'"';
	for (++args; *args && (quoted ? *args != L'"' : *args != L' ' && *args != L'\t'); ++args)
	{
	}
	if (quoted && *args)
		++args;

	wchar_t self[MAX_PATH];
	DWORD n = GetModuleFileNameW (nullptr, self, MAX_PATH);
	std::wstring exe (self, n);
	exe = exe.substr (0, exe.find_last_of (L"\\/") + 1) + L"chomp-cli.exe";

	// stderr, so the JSON on stdout stays clean for scripts
	fwprintf (stderr,
	    L"cement-cli: this is a stub that runs chomp-cli.exe. cement-cli will be removed in a later build: "
	    L"use chomp-cli instead.\n");

	std::wstring cmd = L"\"" + exe + L"\"" + args;
	STARTUPINFOW si{sizeof (si)};
	PROCESS_INFORMATION pi{};
	if (!CreateProcessW (exe.c_str (), cmd.data (), nullptr, nullptr, TRUE, 0, nullptr, nullptr, &si, &pi))
	{
		fwprintf (stderr, L"cement-cli: chomp-cli.exe not found next to cement-cli.exe (%ls)\n", exe.c_str ());
		return 1;
	}
	WaitForSingleObject (pi.hProcess, INFINITE);
	DWORD code = 1;
	GetExitCodeProcess (pi.hProcess, &code);
	CloseHandle (pi.hProcess);
	CloseHandle (pi.hThread);
	return (int)code;
}
