package app.bookworm

import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

/**
 * Поиск сведений о книге в сети: Google Books, а если там пусто или недоступно — Open Library.
 * Уходит только то, что пользователь сам ввёл в поле поиска (название и автор). Вызывать не из главного потока.
 */
object BookSearch {
    class Hit(val title: String, val author: String, val year: String, val coverUrl: String?)

    fun search(title: String, author: String): List<Hit> {
        val g = runCatching { google(title, author) }.getOrDefault(emptyList())
        if (g.isNotEmpty()) return g
        return openLibrary(title, author)
    }

    private fun google(title: String, author: String): List<Hit> {
        val q = buildList {
            if (title.isNotBlank()) add("intitle:" + title.trim())
            if (author.isNotBlank()) add("inauthor:" + author.trim())
        }.joinToString(" ")
        val url = "https://www.googleapis.com/books/v1/volumes?q=${enc(q)}&maxResults=10&printType=books" +
            "&fields=items(volumeInfo(title,subtitle,authors,publishedDate,imageLinks/thumbnail))"
        val items = JSONObject(get(url)).optJSONArray("items") ?: return emptyList()
        return (0 until items.length()).mapNotNull { i ->
            val v = items.getJSONObject(i).optJSONObject("volumeInfo") ?: return@mapNotNull null
            val t = v.optString("title").ifBlank { return@mapNotNull null }
            val a = v.optJSONArray("authors")?.let { arr -> (0 until arr.length()).joinToString(", ") { arr.getString(it) } }.orEmpty()
            // Ссылка на обложку приходит по http и с загнутым углом; берём https без него.
            val cover = v.optJSONObject("imageLinks")?.optString("thumbnail")?.takeIf { it.isNotBlank() }
                ?.replace("http://", "https://")?.replace("&edge=curl", "")
            Hit(t, a, v.optString("publishedDate").take(4), cover)
        }
    }

    private fun openLibrary(title: String, author: String): List<Hit> {
        val q = buildString {
            if (title.isNotBlank()) append("title=").append(enc(title.trim())).append('&')
            if (author.isNotBlank()) append("author=").append(enc(author.trim())).append('&')
        }
        val url = "https://openlibrary.org/search.json?${q}limit=10&fields=title,author_name,first_publish_year,cover_i"
        val docs = JSONObject(get(url)).optJSONArray("docs") ?: return emptyList()
        return (0 until docs.length()).mapNotNull { i ->
            val d = docs.getJSONObject(i)
            val t = d.optString("title").ifBlank { return@mapNotNull null }
            val a = d.optJSONArray("author_name")?.let { arr -> (0 until arr.length()).joinToString(", ") { arr.getString(it) } }.orEmpty()
            val cover = d.optLong("cover_i", 0).takeIf { it > 0 }?.let { "https://covers.openlibrary.org/b/id/$it-L.jpg" }
            Hit(t, a, d.optInt("first_publish_year", 0).takeIf { it > 0 }?.toString().orEmpty(), cover)
        }
    }

    /** Обложка по ссылке из результата поиска; null — не скачалась. */
    fun cover(url: String): ByteArray? = runCatching { open(url).inputStream.use { it.readBytes() } }.getOrNull()?.takeIf { it.size > 1000 }

    private fun get(url: String): String = open(url).inputStream.use { it.readBytes().decodeToString() }

    private fun open(url: String): HttpURLConnection = (URL(url).openConnection() as HttpURLConnection).apply {
        connectTimeout = 8000
        readTimeout = 10000
        setRequestProperty("User-Agent", "BookWorm/0.5 (personal reader)")
        if (responseCode !in 200..299) error("HTTP $responseCode")
    }

    private fun enc(s: String) = URLEncoder.encode(s, "UTF-8")
}
