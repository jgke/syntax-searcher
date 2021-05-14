(defn do-query-thing [db param]
  (j/query db
           [(str "select * from user where username = '" param "'")]))
