package io.arcvera.core.bootstrap;

import com.sun.jna.Memory;
import com.sun.jna.Native;
import com.sun.jna.Platform;
import com.sun.jna.Pointer;
import com.sun.jna.platform.win32.Advapi32;
import com.sun.jna.platform.win32.Advapi32Util;
import com.sun.jna.platform.win32.Kernel32;
import com.sun.jna.platform.win32.WinBase;
import com.sun.jna.platform.win32.WinDef.DWORD;
import com.sun.jna.platform.win32.WinNT;
import com.sun.jna.ptr.IntByReference;
import com.sun.jna.ptr.PointerByReference;
import com.sun.jna.win32.StdCallLibrary;
import com.sun.jna.win32.W32APIOptions;
import java.io.IOException;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;
import java.util.UUID;

/** Owns an exclusive profile lock and the non-delete-sharing ancestor handles until close. */
public final class WindowsProfileLock implements AutoCloseable {
    private static final int READ_CONTROL = 0x00020000;
    private static final int FILE_READ_ATTRIBUTES = 0x80;
    private static final int SHARE_READ_WRITE = 3;
    private static final int OPEN_EXISTING = 3;
    private static final int OPEN_ALWAYS = 4;
    private static final int OPEN_REPARSE = 0x00200000;
    private static final int BACKUP_SEMANTICS = 0x02000000;
    private static final int ATTR_DIRECTORY = 0x10;
    private static final int ATTR_REPARSE = 0x400;
    private static final int FILE_ATTRIBUTE_TAG_INFO = 9;
    private static final int FILE_STANDARD_INFO = 1;
    private static final int FILE_ID_INFO = 18;
    private static final int FILE_DISPOSITION_INFO = 4;
    private final List<WinNT.HANDLE> handles = new ArrayList<>();
    private final Path directory;
    private final String sid;
    private final UUID profileId;
    private boolean closed;
    private byte[] ownedId;
    private byte[] ownedDigest;

    public interface SecurityApi extends StdCallLibrary {
        boolean ConvertStringSecurityDescriptorToSecurityDescriptorW(String sddl, int revision,
                PointerByReference descriptor, IntByReference size);
    }

    public WindowsProfileLock(Path home, UUID profileId) throws IOException {
        Objects.requireNonNull(home);
        Objects.requireNonNull(profileId);
        this.profileId = profileId;
        if (!Platform.isWindows()) throw new IOException("Windows profile protection unavailable");
        if (!home.isAbsolute() || !home.normalize().equals(home)) throw new IOException("Home must be an absolute normalized path");
        sid = currentSid();
        directory = home.resolve("runtime").resolve("core2").resolve(profileId.toString());
        Pointer security = null;
        try {
            var api = Native.load("Advapi32", SecurityApi.class, W32APIOptions.UNICODE_OPTIONS);
            var descriptor = new PointerByReference();
            String sddl = "O:" + sid + "D:P(A;;FA;;;" + sid + ")(A;;FA;;;SY)";
            if (!api.ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl, 1, descriptor, null)) throw error("Create security descriptor");
            security = descriptor.getValue();
            var attributes = new WinBase.SECURITY_ATTRIBUTES();
            attributes.dwLength = new DWORD(attributes.size());
            attributes.lpSecurityDescriptor = security;
            attributes.bInheritHandle = false;
            attributes.write();

            Path part = home.getRoot();
            holdDirectory(part, false);
            for (Path name : home) {
                part = part.resolve(name);
                holdDirectory(part, false);
            }
            for (String name : List.of("runtime", "core2", profileId.toString())) {
                part = part.resolve(name);
                if (!Kernel32.INSTANCE.CreateDirectory(part.toString(), attributes)
                        && Kernel32.INSTANCE.GetLastError() != 183) throw error("Create protected directory");
                holdDirectory(part, true);
            }
            WinNT.HANDLE lock = Kernel32.INSTANCE.CreateFile(directory.resolve("profile.lock").toString(),
                    WinNT.GENERIC_READ | WinNT.GENERIC_WRITE | READ_CONTROL, 0, attributes,
                    OPEN_ALWAYS, OPEN_REPARSE, null);
            if (WinBase.INVALID_HANDLE_VALUE.equals(lock)) throw error("Acquire profile lock");
            handles.add(lock);
            verifyType(lock, false);
            verifySecurity(lock);
            try (Memory standard = new Memory(24)) {
                if (!Kernel32.INSTANCE.GetFileInformationByHandleEx(lock, FILE_STANDARD_INFO, standard, new DWORD(24))) throw error("Read lock file information");
                if (standard.getLong(8) != 0 || standard.getInt(16) != 1 || standard.getByte(20) != 0) {
                    throw new IOException("Profile lock is not an empty single-link file");
                }
            }
        } catch (IOException | RuntimeException | Error failure) {
            try { close(); } catch (IOException closeFailure) { failure.addSuppressed(closeFailure); }
            throw failure;
        } finally {
            if (security != null) Kernel32.INSTANCE.LocalFree(security);
        }
    }

    public Path directory() { return directory; }
    public String sid() { return sid; }
    public UUID profileId() { return profileId; }

    /** Publishes only a new file or this lock holder's last verified file. */
    synchronized void publish(byte[] contents) throws IOException {
        Objects.requireNonNull(contents);
        if (closed) throw new CredentialRegistry.PublicationNotCommittedException("Profile lock closed", null);
        if (contents.length == 0 || contents.length > 65536) {
            throw new CredentialRegistry.PublicationNotCommittedException("Descriptor size rejected", null);
        }
        Path target = directory.resolve("connection.json");
        Path temporary = directory.resolve(".connection-" + UUID.randomUUID() + ".tmp");
        Pointer security = null;
        boolean created = false;
        boolean moveAttempted = false;
        byte[] tempId;
        Throwable primary = null;
        try {
            var existing = inspect(target, true);
            if (ownedId == null ? existing != null : existing == null || !Arrays.equals(ownedId, existing.id)
                    || !MessageDigest.isEqual(ownedDigest, existing.digest)) {
                throw new IOException("Descriptor conflict");
            }
            var api = Native.load("Advapi32", SecurityApi.class, W32APIOptions.UNICODE_OPTIONS);
            var descriptor = new PointerByReference();
            String sddl = "O:" + sid + "D:P(A;;FA;;;" + sid + ")(A;;FA;;;SY)";
            if (!api.ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl, 1, descriptor, null)) throw error("Create security descriptor");
            security = descriptor.getValue();
            var attributes = new WinBase.SECURITY_ATTRIBUTES();
            attributes.dwLength = new DWORD(attributes.size());
            attributes.lpSecurityDescriptor = security;
            attributes.bInheritHandle = false;
            attributes.write();
            var handle = Kernel32.INSTANCE.CreateFile(temporary.toString(), WinNT.GENERIC_READ | WinNT.GENERIC_WRITE | READ_CONTROL,
                    0, attributes, 1, OPEN_REPARSE, null);
            if (WinBase.INVALID_HANDLE_VALUE.equals(handle)) throw error("Create protected temporary descriptor");
            created = true;
            try {
                verifyType(handle, false);
                verifySecurity(handle);
                verifySingleLink(handle);
                tempId = fileId(handle);
                var written = new IntByReference();
                if (!Kernel32.INSTANCE.WriteFile(handle, contents, contents.length, written, null) || written.getValue() != contents.length) {
                    throw error("Write descriptor");
                }
                if (!Kernel32.INSTANCE.FlushFileBuffers(handle)) throw error("Flush descriptor");
            } finally {
                if (!Kernel32.INSTANCE.CloseHandle(handle)) throw error("Close descriptor temporary file");
            }
            moveAttempted = true;
            if (!Kernel32.INSTANCE.MoveFileEx(temporary.toString(), target.toString(), new DWORD(9))) throw error("Replace descriptor");
            created = false;
            var published = inspect(target, false);
            if (!Arrays.equals(tempId, published.id) || !MessageDigest.isEqual(digest(contents), published.digest)) throw new IOException("Published descriptor mismatch");
            ownedId = published.id;
            ownedDigest = published.digest;
        } catch (IOException failure) {
            IOException result = moveAttempted ? failure
                    : new CredentialRegistry.PublicationNotCommittedException("Descriptor was not replaced", failure);
            primary = result;
            throw result;
        } catch (RuntimeException | Error failure) {
            primary = failure;
            throw failure;
        } finally {
            IOException cleanup = null;
            if (created && !Kernel32.INSTANCE.DeleteFile(temporary.toString()) && Kernel32.INSTANCE.GetLastError() != 2)
                cleanup = error("Remove unpublished descriptor temporary file");
            if (security != null) Kernel32.INSTANCE.LocalFree(security);
            if (cleanup != null) {
                if (primary != null) primary.addSuppressed(cleanup); else throw cleanup;
            }
        }
    }

    private record FileObservation(byte[] id, byte[] digest) { }

    synchronized void withdraw() throws IOException {
        if (closed) throw new IOException("Profile lock closed");
        if (ownedId == null) return;
        var handle = Kernel32.INSTANCE.CreateFile(directory.resolve("connection.json").toString(),
                WinNT.GENERIC_READ | READ_CONTROL | 0x00010000, 7, null, OPEN_EXISTING, OPEN_REPARSE, null);
        if (WinBase.INVALID_HANDLE_VALUE.equals(handle)) throw error("Open owned descriptor for withdrawal");
        try {
            FileObservation current = observe(handle);
            if (!Arrays.equals(ownedId, current.id) || !MessageDigest.isEqual(ownedDigest, current.digest)) {
                throw new IOException("Descriptor changed; withdrawal refused");
            }
            try (Memory disposition = new Memory(1)) {
                disposition.setByte(0, (byte) 1);
                if (!Kernel32.INSTANCE.SetFileInformationByHandle(handle, FILE_DISPOSITION_INFO, disposition, new DWORD(1))) {
                    throw error("Withdraw descriptor");
                }
            }
        } finally {
            if (!Kernel32.INSTANCE.CloseHandle(handle)) throw error("Close withdrawn descriptor");
        }
        ownedId = null;
        ownedDigest = null;
    }

    private FileObservation inspect(Path path, boolean allowMissing) throws IOException {
        var handle = Kernel32.INSTANCE.CreateFile(path.toString(), WinNT.GENERIC_READ | READ_CONTROL,
                7, null, OPEN_EXISTING, OPEN_REPARSE, null);
        if (WinBase.INVALID_HANDLE_VALUE.equals(handle)) {
            if (allowMissing && Kernel32.INSTANCE.GetLastError() == 2) return null;
            throw error("Open descriptor");
        }
        try {
            return observe(handle);
        } finally {
            if (!Kernel32.INSTANCE.CloseHandle(handle)) throw error("Close descriptor");
        }
    }

    private FileObservation observe(WinNT.HANDLE handle) throws IOException {
        verifyType(handle, false);
        verifySecurity(handle);
        int length = verifySingleLink(handle);
        if (length < 1 || length > 65536) throw new IOException("Descriptor size rejected");
        byte[] bytes = new byte[length];
        try {
            var read = new IntByReference();
            if (!Kernel32.INSTANCE.ReadFile(handle, bytes, length, read, null) || read.getValue() != length) throw error("Read descriptor");
            return new FileObservation(fileId(handle), digest(bytes));
        } finally { Arrays.fill(bytes, (byte) 0); }
    }

    private static byte[] fileId(WinNT.HANDLE handle) throws IOException {
        try (Memory identity = new Memory(24)) {
            if (!Kernel32.INSTANCE.GetFileInformationByHandleEx(handle, FILE_ID_INFO, identity, new DWORD(24))) throw error("Read descriptor identity");
            return identity.getByteArray(0, 24);
        }
    }

    private static int verifySingleLink(WinNT.HANDLE handle) throws IOException {
        try (Memory info = new Memory(24)) {
            if (!Kernel32.INSTANCE.GetFileInformationByHandleEx(handle, FILE_STANDARD_INFO, info, new DWORD(24))) throw error("Read file information");
            long length = info.getLong(8);
            if (info.getInt(16) != 1 || info.getByte(20) != 0 || length < 0 || length > 65536) {
                throw new IOException("Descriptor is not a bounded single-link file");
            }
            return (int) length;
        }
    }

    private static byte[] digest(byte[] contents) {
        try { return MessageDigest.getInstance("SHA-256").digest(contents); }
        catch (NoSuchAlgorithmException e) { throw new IllegalStateException("SHA-256 unavailable", e); }
    }

    private void holdDirectory(Path path, boolean protectedDirectory) throws IOException {
        WinNT.HANDLE handle = Kernel32.INSTANCE.CreateFile(path.toString(), READ_CONTROL | FILE_READ_ATTRIBUTES,
                SHARE_READ_WRITE, null, OPEN_EXISTING, OPEN_REPARSE | BACKUP_SEMANTICS, null);
        if (WinBase.INVALID_HANDLE_VALUE.equals(handle)) throw error("Open directory");
        handles.add(handle);
        verifyType(handle, true);
        if (protectedDirectory) verifySecurity(handle);
    }

    private static void verifyType(WinNT.HANDLE handle, boolean directory) throws IOException {
        try (Memory info = new Memory(8)) {
            if (!Kernel32.INSTANCE.GetFileInformationByHandleEx(handle, FILE_ATTRIBUTE_TAG_INFO, info, new DWORD(8))) throw error("Read file attributes");
            int attributes = info.getInt(0);
            if ((attributes & ATTR_REPARSE) != 0 || ((attributes & ATTR_DIRECTORY) != 0) != directory) {
                throw new IOException("Reparse point or unexpected file type");
            }
        }
    }

    private void verifySecurity(WinNT.HANDLE handle) throws IOException {
        var sd = new PointerByReference();
        int result = Advapi32.INSTANCE.GetSecurityInfo(handle, 1, 5, null, null, null, null, sd);
        if (result != 0) throw new IOException("Read security descriptor failed: " + result);
        try {
            var descriptor = new WinNT.SECURITY_DESCRIPTOR_RELATIVE(sd.getValue());
            if ((descriptor.Control & 0x1000) == 0 || descriptor.getOwner() == null
                    || !sid.equals(Advapi32Util.convertSidToStringSid(descriptor.getOwner()))) {
                throw new IOException("Unexpected owner or unprotected DACL");
            }
            var acl = descriptor.getDiscretionaryACL();
            if (acl == null) throw new IOException("Missing DACL");
            var entries = acl.getACEs();
            boolean user = false;
            boolean system = false;
            for (var ace : entries) {
                if (!(ace instanceof WinNT.ACCESS_ALLOWED_ACE allowed) || ace.AceFlags != 0) throw new IOException("Unexpected DACL entry");
                String aceSid = allowed.getSidString();
                if (sid.equals(aceSid)) user = true;
                else if ("S-1-5-18".equals(aceSid)) system = true;
                else throw new IOException("Unexpected DACL principal");
            }
            if (entries.length != 2 || !user || !system) throw new IOException("Incomplete protected DACL");
        } finally {
            Kernel32.INSTANCE.LocalFree(sd.getValue());
        }
    }

    private static String currentSid() throws IOException {
        var token = new WinNT.HANDLEByReference();
        if (!Advapi32.INSTANCE.OpenProcessToken(Kernel32.INSTANCE.GetCurrentProcess(), WinNT.TOKEN_QUERY, token)) throw error("Open process token");
        try { return Advapi32Util.getTokenAccount(token.getValue()).sidString; }
        finally { if (!Kernel32.INSTANCE.CloseHandle(token.getValue())) throw error("Close process token"); }
    }

    private static IOException error(String operation) { return new IOException(operation + " failed with Windows error " + Kernel32.INSTANCE.GetLastError()); }

    @Override public synchronized void close() throws IOException {
        if (closed) return;
        closed = true;
        IOException failure = null;
        for (int i = handles.size() - 1; i >= 0; i--) {
            if (!Kernel32.INSTANCE.CloseHandle(handles.get(i))) {
                IOException next = error("Close profile handle");
                if (failure == null) failure = next; else failure.addSuppressed(next);
            }
        }
        handles.clear();
        if (failure != null) throw failure;
    }
}
