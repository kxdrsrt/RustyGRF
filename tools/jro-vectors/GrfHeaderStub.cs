using System.Collections.Generic;
namespace GRF.Core {
	// Only what the protected-entry decoder uses; the kRO default keys are not needed for jRO.
	public static class GrfHeader {
		public static Dictionary<uint, byte[]> KRODefaultProtectedKeys = new Dictionary<uint, byte[]>();
	}
}
